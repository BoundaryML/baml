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
//! Release is GC/drain-driven. The registry retains each JavaScript function,
//! but its dispatch reference does not keep the event loop alive. Native call
//! promises keep in-flight calls alive, and the beforeExit hook shuts down
//! registered engines and drains background BAML work.

use std::{
    collections::HashMap,
    sync::{
        Arc, LazyLock, Mutex, OnceLock,
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
    ThreadsafeFunction<DispatchArgs, (), DispatchArgs, Status, false, true, DISPATCH_QUEUE_SIZE>;

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
        .weak::<true>()
        // Bound the queue (napi's default is unbounded); see DISPATCH_QUEUE_SIZE.
        .max_queue_size::<DISPATCH_QUEUE_SIZE>()
        .build()?;
    let key = next_key();
    REGISTRY.table.lock().unwrap().insert(key, Arc::new(tsfn));
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
/// Dropping the `ThreadsafeFunction` releases the underlying napi reference
/// (while its libuv reference is weak), allowing the user's JS
/// callable to become GC-eligible and unpinning the event loop. Shared by the
/// engine-driven release path ([`host_release_callback`]) and the encoder's
/// rollback path ([`release_host_callable`]).
fn drop_registry_entry(host_value_key: u64) {
    let popped: Option<Arc<DispatchTsfn>> = match REGISTRY.table.lock() {
        Ok(mut t) => t.remove(&host_value_key),
        Err(e) => {
            // Poisoning means an earlier panic occurred while holding the
            // lock; the table is in an unknown state. Don't try to mutate
            // it (could double-drop), but log so the underlying panic is
            // attributable. We accept the leak: the engine has already
            // dropped its `Arc<HostValueArc>` (we're on the release path),
            // and a poisoned global registry implies the process is in a
            // failing state anyway.
            log::warn!(
                "host-callable registry mutex poisoned during release of key \
                 {host_value_key}: {e}; entry leaked"
            );
            return;
        }
    };
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
    if let Some(tsfn) = HOST_VALUE_RELEASE_CALLBACK.get() {
        // Fire-and-forget: the TS callback removes the map entry on the
        // libuv loop. `QueueFull` would mean an enormous backlog of
        // releases — log it (so it's visible in stress tests) and move
        // on. The dropped Arc has no further engine-side state and a
        // missed map entry just delays JS-error GC by an extra cycle.
        let status = tsfn.call(
            HandleKey::from_u64(host_value_key),
            ThreadsafeFunctionCallMode::NonBlocking,
        );
        if status != Status::Ok {
            log::warn!(
                "host_release_callback: host-value-release tsfn returned {status:?} \
                 for key {host_value_key}; TS-side map entry will leak until next GC",
            );
        }
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
// Release is a fire-and-forget tsfn call on the libuv loop — TS removes the
// entry once napi schedules the callback. A lookup that races a release
// returns the (about-to-be-released) reference, which only delays GC of
// that value by one tick; no correctness issue. The TS map never
// silently leaks: every key minted via `mint_host_value_key` corresponds
// to an `Arc<HostValueArc>` on the engine side whose `Drop` is guaranteed
// to fire the release callback.

/// Threadsafe handle to the TS-installed release callback. Set once at
/// module load via [`register_host_value_release_callback`].
type HostValueReleaseTsfn = ThreadsafeFunction<
    HandleKey,
    (),
    HandleKey,
    Status,
    false,
    true,
    HOST_VALUE_RELEASE_QUEUE_SIZE,
>;

/// Upper bound on queued, not-yet-delivered host-value-release notifications.
/// Generous because each notification is tiny (one `HandleKey`) and bursts
/// can happen during engine GC sweeps. `Status::QueueFull` from
/// `tsfn.call` is logged but not otherwise surfaced — the TS map entry
/// stays until the process exits, but the engine's `HostValueArc` has
/// already dropped so there's no further engine state to clean up.
const HOST_VALUE_RELEASE_QUEUE_SIZE: usize = 4096;

static HOST_VALUE_RELEASE_CALLBACK: OnceLock<Arc<HostValueReleaseTsfn>> = OnceLock::new();

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

/// Install the TS-side release callback. First-call-wins; subsequent
/// calls are a no-op (matching the bridge_cffi dispatch-registration
/// semantics). The callback fires for *every* `HostValueArc` release —
/// for callable keys it's a TS-side no-op (`Map.delete(key)` on an absent
/// key), so Rust doesn't need to distinguish kinds here.
///
/// The tsfn is built with `weak::<true>()` (i.e. `napi_unref_threadsafe_
/// function`). Holding it strong would pin the libuv loop for the
/// lifetime of the process (the tsfn is parked in a `OnceLock` and never
/// dropped), preventing the Node process from exiting even after all
/// host work is done. Weak is correct here: the callback is a *release*
/// notification — purely informational from the engine's side. Pending
/// notifications that never deliver because the loop has already exited
/// are harmless; the engine has already dropped its `Arc<HostValueArc>`,
/// and the TS-side map entry would be torn down with the process
/// anyway.
///
/// Dispatch callbacks also use weak libuv references. Pending native call
/// promises keep the loop alive; the beforeExit shutdown hook drains spawned
/// BAML work. Idle program registrations must not prevent that hook from running.
///
/// Exposed to JS as `registerHostValueReleaseCallback(cb)`. Must be called
/// exactly once at SDK module init, before any host call is dispatched.
#[napi(ts_args_type = "callback: (key: HandleKey) => void")]
pub fn register_host_value_release_callback(
    callback: Function<'_, HandleKey, ()>,
) -> napi::Result<()> {
    let tsfn: HostValueReleaseTsfn = callback
        .build_threadsafe_function()
        .callee_handled::<false>()
        .weak::<true>()
        .max_queue_size::<HOST_VALUE_RELEASE_QUEUE_SIZE>()
        .build()?;
    // First-call-wins; ignore the `Err(_)` from `set` on later calls
    // (caller is responsible for not re-registering).
    let _ = HOST_VALUE_RELEASE_CALLBACK.set(Arc::new(tsfn));
    Ok(())
}

/// Release a host callable the inbound encoder registered but never handed to
/// the engine — the encode-error rollback path.
///
/// Exposed to JS as `releaseHostCallable(key)`. When `encodeCallArgs`
/// registers a callable for an early kwarg and then fails to encode a later
/// kwarg, the `CallFunctionArgs` is never sent, so the engine never decodes
/// (and so never releases) that key. Without this, the registry entry — and
/// its retained JavaScript function reference —
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
    let tsfn: Option<Arc<DispatchTsfn>> = match REGISTRY.table.lock() {
        Ok(t) => t.get(&host_value_key).cloned(),
        Err(e) => {
            // Treat poisoning as a "no callable" condition for this
            // dispatch (the engine call must still complete), but log so
            // the originating panic is attributable instead of being
            // swallowed silently as an opaque `no-callable` error.
            log::warn!(
                "host-callable registry mutex poisoned during dispatch of key \
                 {host_value_key}: {e}; treating as no-callable"
            );
            None
        }
    };
    let Some(tsfn) = tsfn else {
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
