//! Per-process Node.js host-value registry.
//!
//! When JavaScript passes a callable as an argument to a BAML function, the
//! inbound encoder (`typescript_src/proto.ts`) calls
//! [`register_host_callable`] (exposed as `registerHostCallable`) with a
//! small JS *dispatch wrapper* that knows how to:
//!
//!   1. Decode the engine-side `BamlOutboundValue` args (the wire payload is
//!      a list shape — see `sys_native::host_impls::call_host_value`) into
//!      JS positional arguments.
//!   2. Invoke the user callable, awaiting its `Promise` when it returns one.
//!   3. Encode the result as an `InboundValue` and invoke
//!      [`complete_host_call`] (the napi-exposed wrapper around the C
//!      `bridge_cffi::complete_host_call`).
//!
//! `register_host_callable` returns a `HandleKey` (a `{low, high}` u64
//! split, matching the rest of the Node bridge) and stores the dispatch
//! wrapper as a [`ThreadsafeFunction`] in a process-wide registry. The TS
//! encoder emits `InboundValue::Handle { key, handle_type: HOST_VALUE_CALLABLE }`
//! using that key.
//!
//! From Rust's side, when BAML invokes the host value, the `call_host_value`
//! sysop calls the registered [`host_dispatch_callback`] which looks the
//! `ThreadsafeFunction` up by key and schedules a call onto the JS event
//! loop with `(callId, argsBytes)`. The JS dispatch wrapper completes the
//! call via [`complete_host_call`].
//!
//! When the engine drops its last clone of the corresponding `HostValueArc`,
//! [`host_release_callback`] fires and removes the registry entry — the
//! `ThreadsafeFunction`'s `Drop` releases the underlying JS reference, which
//! lets the user's callable become GC-eligible.
//!
//! Release is therefore GC/drain-driven: the entry — and its strong
//! (`weak::<false>`) tsfn ref, which pins the libuv loop — lives until the
//! engine collects or drops the owning `Object::HostClosure` and the deferred
//! release is drained (`host_release_dispatch::drain`, run at GC safepoints
//! and after each call). A callable that is never collected before the
//! process tears down keeps its ref, which is why the Node test suite runs
//! jest with `forceExit`. A teardown-time drain (releasing every still-live
//! host value when a runtime is dropped) would close that gap but depends on
//! heap-teardown semantics owned by the engine/heap layer; it is left to that
//! layer rather than worked around here.

use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc, LazyLock, Mutex, MutexGuard, OnceLock, PoisonError,
        atomic::{AtomicU64, Ordering},
    },
};

use napi::{
    Status,
    bindgen_prelude::{Buffer, FnArgs, Function},
    threadsafe_function::{ThreadsafeFunction, ThreadsafeFunctionCallMode},
};
use napi_derive::napi;
use sys_native::{OpError, SysOp, VmInternalError};

use crate::handle::HandleKey;

/// Args the Rust-side dispatch forwards to the JS dispatch wrapper.
///
/// Wrapped in `FnArgs` because napi-rs's `JsValuesTupleIntoVec` is impl'd
/// on `FnArgs<(...)>`, not raw tuples.
type DispatchArgs = FnArgs<(u32, Buffer)>;

/// Type of the per-callable dispatch wrapper held in the registry.
///
/// - `T = DispatchArgs`: the data we pass to the JS function on each call.
/// - `Return = ()`: the JS dispatch wrapper completes the call via
///   `complete_host_call`; its return value is ignored.
/// - `CalleeHandled = false`: the JS wrapper is responsible for catching
///   its own errors and reporting them via `complete_host_call`; we don't
///   want napi-rs to interpret a Result on the Rust side.
/// - `Weak = false` / `MaxQueueSize = DISPATCH_QUEUE_SIZE`: a strong ref with a
///   bounded queue (see [`DISPATCH_QUEUE_SIZE`]).
type DispatchTsfn =
    ThreadsafeFunction<DispatchArgs, (), DispatchArgs, Status, false, false, DISPATCH_QUEUE_SIZE>;

/// Upper bound on queued, not-yet-delivered host-call dispatches per callable.
///
/// napi's default queue size is `0` (unbounded); with the `NonBlocking`
/// dispatch a backlog could grow without limit if JS can't keep up. A full
/// queue makes `tsfn.call` return `Status::QueueFull`, which the dispatch site
/// surfaces as a `HostCallable` error (see `send_dispatch_error_tsfn_status`)
/// rather than dropping the call silently.
const DISPATCH_QUEUE_SIZE: usize = 1024;

/// Process-wide table of JS dispatch wrappers handed to BAML.
///
/// The key is a freshly-allocated `u64` (never 0). Removal happens in
/// [`host_release_callback`] when Rust drops its last clone of the
/// corresponding `HostValueArc`.
struct Registry {
    next_key: AtomicU64,
    // `ThreadsafeFunction` does not implement `Clone`, so we wrap in `Arc`
    // to allow the dispatch path to take a cheap reference-counted handle
    // out of the table without holding the registry mutex while we schedule
    // the JS call.
    table: Mutex<HashMap<u64, Arc<DispatchTsfn>>>,
}

static REGISTRY: LazyLock<Registry> = LazyLock::new(|| Registry {
    next_key: AtomicU64::new(1),
    table: Mutex::new(HashMap::new()),
});

/// Lock the dispatch table, recovering from poisoning: the map holds only
/// primitive keys and `Arc`s and no user code runs under the lock, so it is
/// structurally intact after an unwind and refusing to touch it would only
/// leak entries.
fn dispatch_table() -> MutexGuard<'static, HashMap<u64, Arc<DispatchTsfn>>> {
    REGISTRY
        .table
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

fn next_key() -> u64 {
    loop {
        let k = REGISTRY.next_key.fetch_add(1, Ordering::Relaxed);
        if k != 0 {
            return k;
        }
    }
}

/// Register a JS dispatch wrapper in the host-value table and return its key.
///
/// Exposed to JS as `registerHostCallable(fn) -> HandleKey`. Called from
/// the inbound encoder in `typescript_src/proto.ts` whenever a JS callable
/// appears as a kwarg — the encoder constructs the dispatch wrapper around
/// the user's function before calling this.
///
/// The `Function` is converted into a `ThreadsafeFunction` so it can outlive
/// the napi call scope and be invoked from any thread (the engine's tokio
/// runtime calls into this entry point from a worker thread).
#[napi(ts_args_type = "callable: (callId: number, argsBytes: Buffer) => void")]
pub fn register_host_callable(callable: Function<'_, DispatchArgs, ()>) -> napi::Result<HandleKey> {
    let tsfn: DispatchTsfn = callable
        .build_threadsafe_function()
        .callee_handled::<false>()
        .weak::<false>()
        // Bound the queue (napi's default is unbounded); see DISPATCH_QUEUE_SIZE.
        .max_queue_size::<DISPATCH_QUEUE_SIZE>()
        .build()?;
    let key = next_key();
    dispatch_table().insert(key, Arc::new(tsfn));
    Ok(HandleKey::from_u64(key))
}

/// Complete an in-flight host call from the JS dispatch wrapper.
///
/// Exposed to JS as `completeHostCall(callId, isError, content)`. The JS
/// dispatch wrapper invokes this after it has decoded `argsBytes`, called
/// the user function, and encoded the result as an `InboundValue` (success
/// is the value itself; an error is an `Instance` of
/// `baml.errors.HostCallable` carrying the four metadata fields).
///
/// Forwards directly to the `bridge_cffi::complete_host_call` C entry point
/// the engine uses for cross-language completion.
#[napi(js_name = "completeHostCall")]
pub fn complete_host_call(call_id: u32, is_error: i32, content: Buffer) {
    bridge_cffi::complete_host_call(
        call_id,
        is_error,
        content.as_ptr() as *const i8,
        content.len(),
    );
}

/// Remove and drop the registry entry for `host_value_key` (if present).
///
/// Dropping the `ThreadsafeFunction` releases the underlying napi reference,
/// allowing the user's JS callable to become GC-eligible. Shared by the
/// engine-driven release path ([`host_release_callback`]) and the encoder's
/// rollback path ([`release_host_callable`]). The entry is dropped after the
/// table lock is released so the napi teardown never runs under it.
fn drop_registry_entry(host_value_key: u64) {
    let popped = dispatch_table().remove(&host_value_key);
    drop(popped);
}

/// Drop the JS dispatch wrapper / host-value-map entry associated with
/// `host_value_key`.
///
/// Fires when the last Rust clone of the corresponding `HostValueArc` is
/// dropped — see `bex_external_types::host_value::host_release_dispatch`.
/// We don't track *which* kind (callable vs opaque) the key referred to:
/// every release attempts both the Rust-side callable drop and the
/// TS-side host-value-map delete. Whichever one of the two registries actually
/// held the entry cleans it up; the other is a benign no-op.
pub extern "C" fn host_release_callback(host_value_key: u64) {
    drop_registry_entry(host_value_key);
    if let Some(channel) = HOST_VALUE_RELEASE.get() {
        channel.enqueue(host_value_key);
    }
}

// ============================================================================
// Host-value registry — JS-side storage with Rust-driven release
// ============================================================================
//
// An arbitrary host JS value (e.g. a native exception raised inside a user
// callback) round-trips back to the same Node process as the *same* object
// (mirrors the Python bridge's `register_host_opaque` / `lookup_host_value`
// pair). The TS bridge owns the storage (a `Map<bigint, unknown>` of JS
// values) because napi-rs has no zero-overhead persistent reference type for
// arbitrary JS values; Rust owns the key minting (so callable + opaque keys
// share a single globally-unique counter and never collide) and the release
// signal (the engine's `host_release_dispatch::fire(key)` fires Rust's
// `host_release_callback`, which notifies TS to remove its map entry).
//
// Release notifications are coalesced rather than queued one per key. The
// engine's GC drain releases host values in bursts far larger than any
// bounded tsfn queue, and a notification dropped on `QueueFull` would leak
// the TS map entry for the life of the process. So [`host_release_callback`]
// only parks the key in the [`ReleaseChannel`] and makes sure a single
// zero-payload wakeup is on its way; on the JS thread the wakeup pulls keys in
// batches of [`RELEASE_BATCH`], and a batch is forgotten only once the TS
// callback returned normally (a throw keeps it for the next wakeup). The
// wakeup tsfn is unbounded — `QueueFull` cannot happen — and weak, so the
// channel never keeps Node alive; a wakeup lost at teardown is harmless
// because the TS map dies with the process. A lookup that races a release
// returns the (about-to-be-released) reference, which only delays GC of that
// value; no correctness issue. The TS map never silently leaks: every key
// minted via `mint_host_value_key` corresponds to an `Arc<HostValueArc>` on
// the engine side whose `Drop` is guaranteed to fire the release callback.

/// Keys handed to the TS release callback per wakeup.
const RELEASE_BATCH: usize = 1024;

/// The wakeup carries no payload (`T = ()`); the JS-thread transform installed
/// by [`register_host_value_release_callback`] turns it into the next batch.
type ReleaseWakeupTsfn = ThreadsafeFunction<(), (), Vec<HandleKey>, Status, false, true>;

#[derive(Default)]
struct PendingReleases {
    keys: HashSet<u64>,
    /// The batch currently handed to JS. Stays in `keys` until acknowledged.
    in_flight: Vec<u64>,
    /// A wakeup is queued or being delivered; `enqueue` must not post another.
    scheduled: bool,
}

/// Coalescing release channel from the engine to the TS host-value map.
struct ReleaseChannel {
    wakeup: ReleaseWakeupTsfn,
    pending: Mutex<PendingReleases>,
}

/// Installed once at SDK module init by [`register_host_value_release_callback`].
static HOST_VALUE_RELEASE: OnceLock<ReleaseChannel> = OnceLock::new();

impl ReleaseChannel {
    fn pending(&self) -> MutexGuard<'_, PendingReleases> {
        self.pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Any thread: park `key` and ensure exactly one wakeup is in flight.
    fn enqueue(&self, key: u64) {
        let mut pending = self.pending();
        pending.keys.insert(key);
        if pending.scheduled {
            return;
        }
        pending.scheduled = true;
        drop(pending);
        self.wake();
    }

    /// Post the zero-payload wakeup. Caller has set `scheduled`.
    fn wake(&self) {
        let status = self.wakeup.call_with_return_value(
            (),
            ThreadsafeFunctionCallMode::NonBlocking,
            |delivered, _env| {
                if let Some(channel) = HOST_VALUE_RELEASE.get() {
                    channel.acknowledge(delivered);
                }
                Ok(())
            },
        );
        if status != Status::Ok {
            // `Closing` is env teardown, where the map dies anyway. Anything
            // else is unexpected for an unbounded queue; clear `scheduled` so
            // the next release retries instead of parking forever.
            if status != Status::Closing {
                log::warn!("host-value release wakeup failed with status {status:?}");
            }
            self.pending().scheduled = false;
        }
    }

    /// JS thread: the batch for the wakeup being delivered.
    fn next_batch(&self) -> Vec<HandleKey> {
        let mut pending = self.pending();
        let batch: Vec<u64> = pending.keys.iter().copied().take(RELEASE_BATCH).collect();
        pending.in_flight.clone_from(&batch);
        batch.into_iter().map(HandleKey::from_u64).collect()
    }

    /// JS thread: the TS callback returned. Forget the batch on success, keep
    /// it for a retry on a throw, and re-wake while anything remains.
    fn acknowledge(&self, delivered: napi::Result<()>) {
        let mut pending = self.pending();
        let in_flight = std::mem::take(&mut pending.in_flight);
        match delivered {
            Ok(()) => {
                for key in in_flight {
                    pending.keys.remove(&key);
                }
            }
            Err(err) => log::warn!("host-value release callback threw: {err}; batch will be retried"),
        }
        if pending.keys.is_empty() {
            pending.scheduled = false;
            return;
        }
        drop(pending);
        self.wake();
    }
}

/// Mint a fresh host-value key, drawing from the shared callable+opaque
/// counter so the engine sees one globally-unique keyspace. Returned to
/// TS by `registerHostOpaque` (the TS-side function in
/// `host_value_registry.ts`).
///
/// Exposed to JS as `mintHostValueKey() -> HandleKey`. The TS-side host-value
/// registry calls this once per `registerHostOpaque(value)` before inserting
/// the value into its `Map<bigint, unknown>`.
#[napi(js_name = "mintHostValueKey")]
pub fn mint_host_value_key() -> HandleKey {
    HandleKey::from_u64(next_key())
}

/// Install the TS-side batch release callback. First-call-wins; subsequent
/// calls are a no-op (matching the bridge_cffi dispatch-registration
/// semantics). The callback receives every released key — for callable keys
/// it's a TS-side no-op (`Map.delete` on an absent key), so Rust doesn't need
/// to distinguish kinds here — and must be idempotent, since a batch whose
/// delivery threw is handed over again on the next wakeup.
///
/// The wakeup tsfn is weak (`napi_unref_threadsafe_function`): it is parked
/// in a `OnceLock` for the life of the process, and holding it strong would
/// keep Node from exiting after all host work is done. A release is purely
/// informational from the engine's side, so a wakeup that never delivers
/// because the loop already exited is harmless.
///
/// Exposed to JS as `registerHostValueReleaseCallback(cb)`. Must be called
/// exactly once at SDK module init, before any host call is dispatched.
#[napi(ts_args_type = "callback: (keys: Array<HandleKey>) => void")]
pub fn register_host_value_release_callback(callback: Function<'_, (), ()>) -> napi::Result<()> {
    let wakeup: ReleaseWakeupTsfn = callback
        .build_threadsafe_function()
        .callee_handled::<false>()
        .weak::<true>()
        // Runs on the JS thread for each delivered wakeup: pick the batch.
        .build_callback(|_wakeup| {
            Ok(HOST_VALUE_RELEASE
                .get()
                .map(ReleaseChannel::next_batch)
                .unwrap_or_default())
        })?;
    // First-call-wins; ignore the `Err(_)` from `set` on later calls
    // (caller is responsible for not re-registering).
    let _ = HOST_VALUE_RELEASE.set(ReleaseChannel {
        wakeup,
        pending: Mutex::default(),
    });
    Ok(())
}

/// Release a host callable the inbound encoder registered but never handed to
/// the engine — the encode-error rollback path.
///
/// Exposed to JS as `releaseHostCallable(key)`. When `encodeCallArgs`
/// registers a callable for an early kwarg and then fails to encode a later
/// kwarg, the `CallFunctionArgs` is never sent, so the engine never decodes
/// (and so never releases) that key. Without this, the registry entry — and
/// its strong `weak::<false>` tsfn ref, which keeps the libuv loop alive —
/// would leak for the life of the process. The encoder calls this for every
/// key it registered during a failed encode.
#[napi(js_name = "releaseHostCallable")]
pub fn release_host_callable(key: HandleKey) {
    drop_registry_entry(key.to_u64());
}

/// Dispatch a BAML→host call into JavaScript.
///
/// `args` is a protobuf-encoded `BamlOutboundValue` whose variant is a
/// `BamlValueList` (see `sys_native::host_impls::call_host_value` — args
/// are wrapped as `BexExternalValue::Array` before encoding). The JS
/// dispatch wrapper is responsible for decoding the list, calling the user
/// function, and reporting the result via `complete_host_call`.
#[expect(
    clippy::not_unsafe_ptr_arg_deref,
    reason = "C ABI entry point: pointer validity is the caller's contract, documented \
              alongside the registered HostDispatchFn signature in bridge_cffi"
)]
pub extern "C" fn host_dispatch_callback(
    host_value_key: u64,
    call_id: u32,
    args: *const u8,
    length: usize,
) {
    // Copy the wire bytes into a Vec — the dispatch task may outlive the
    // caller's stack frame (the tsfn schedules onto libuv asynchronously),
    // and we need a `'static` slice anyway.
    let bytes: Vec<u8> = if length == 0 || args.is_null() {
        Vec::new()
    } else {
        // SAFETY: the engine guarantees `args` is valid for `length` bytes
        // for the duration of this call (see `sys_native::host_dispatch::fire_dispatch`).
        unsafe { std::slice::from_raw_parts(args, length) }.to_vec()
    };

    // Look up the dispatch wrapper. The `Arc::clone` is cheap (refcount bump);
    // we drop the registry mutex before scheduling the JS call so a long
    // dispatch never blocks `register_host_callable` / `host_release_callback`.
    let Some(tsfn) = dispatch_table().get(&host_value_key).cloned() else {
        send_dispatch_error_no_callable(call_id, host_value_key);
        return;
    };

    // Schedule the JS dispatch wrapper. `NonBlocking` matches the Python
    // bridge's `Handle::spawn` semantics — control returns to the engine
    // promptly and the JS-side work happens on the libuv loop.
    let status = tsfn.call(
        FnArgs::from((call_id, Buffer::from(bytes))),
        ThreadsafeFunctionCallMode::NonBlocking,
    );
    if status != Status::Ok {
        send_dispatch_error_tsfn_status(call_id, status);
    }
}

/// Surface a "no registered JS callable for this host-value key" as a
/// fatal `BridgeFailure` — this isn't a host-language exception, it's a
/// bridge-layer fault (the bridge couldn't find the callable to dispatch
/// to, so the call never reached JS). Routes directly through
/// `host_dispatch::complete_with_error` so the engine sees a
/// `VmInternalError::BridgeFailure`, which surfaces host-side as the
/// engine's existing internal-error path rather than masquerading as a
/// catchable `baml.errors.HostCallable`.
fn send_dispatch_error_no_callable(call_id: u32, host_value_key: u64) {
    sys_native::host_dispatch::complete_with_error(
        call_id,
        OpError::new(
            SysOp::BamlHostCallHostValue,
            VmInternalError::BridgeFailure {
                message: format!("no host callable registered for key {host_value_key}"),
            },
        ),
    );
}

/// Surface a tsfn-scheduling failure (queue full / aborted / library
/// shutdown) as a fatal `BridgeFailure` — the bridge couldn't even
/// schedule the dispatch onto the libuv loop, so the user callable never
/// ran. Same routing rationale as [`send_dispatch_error_no_callable`].
fn send_dispatch_error_tsfn_status(call_id: u32, status: Status) {
    sys_native::host_dispatch::complete_with_error(
        call_id,
        OpError::new(
            SysOp::BamlHostCallHostValue,
            VmInternalError::BridgeFailure {
                message: format!("threadsafe_function call failed with status {status:?}"),
            },
        ),
    );
}
