//! Per-process Node.js host-value registry.
//!
//! When JavaScript passes a callable as an argument to a BAML function, the
//! bridge's argument walker (`crate::encode`) binds it to the function slot
//! it fits and calls `register_callable` with the small JS *dispatch
//! wrapper* the runtime's host adapter hands back, which knows how to:
//!
//!   1. Decode the engine-side `BamlOutboundValue` args (the wire payload is
//!      a list shape — see `sys_native::host_impls::call_host_value`) into
//!      JS positional arguments.
//!   2. Invoke the user callable, awaiting its `Promise` when it returns one.
//!   3. Hand the result to [`complete_host_call_value`], which encodes it
//!      against the bound slot's declared return type and completes the call
//!      through the C `bridge_cffi::complete_host_call`.
//!
//! `register_callable` returns the key and stores the dispatch wrapper as a
//! [`ThreadsafeFunction`] in a process-wide registry, together with the
//! function type it was bound to. The walker emits
//! `InboundValue::Handle { key, handle_type: HOST_VALUE_CALLABLE }` using
//! that key.
//!
//! From Rust's side, when BAML invokes the host value, the `call_host_value`
//! sysop calls the registered [`host_dispatch_callback`] which looks the
//! `ThreadsafeFunction` up by key and schedules a call onto the JS event
//! loop with `(callId, argsBytes)`. The JS dispatch wrapper completes the
//! call with what the callback did: [`complete_host_call_value`],
//! [`complete_host_call_throw`], [`complete_host_call_raised`] or
//! [`complete_host_call_bridge_failure`].
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
    sync::{Arc, LazyLock, Mutex, OnceLock},
};

use bridge_ctypes::{
    HostKey,
    baml_bridge::cffi::{BamlHandleType, BamlToHostCall, HostCallCompletion},
};
use bridge_encode::{Completion, Encoded};
use napi::{
    Env, Status,
    bindgen_prelude::{Buffer, External, FnArgs, Function, Unknown},
    threadsafe_function::{ThreadsafeFunction, ThreadsafeFunctionCallMode},
};
use napi_derive::napi;
use prost::Message;
use sys_native::{OpError, SysOp, VmInternalError};

use crate::handle::HandleKey;

static SYNC_CALLS: LazyLock<Mutex<HashSet<u64>>> = LazyLock::new(Mutex::default);

/// A native closure can hide host callbacks from the JS encoder's sync guard.
/// Track the issuing entry so dispatch can reject before queuing blocked JS.
pub(crate) struct SyncCallGuard(u64);

impl SyncCallGuard {
    pub(crate) fn new(call_id: u64) -> Self {
        SYNC_CALLS.lock().unwrap().insert(call_id);
        Self(call_id)
    }
}

impl Drop for SyncCallGuard {
    fn drop(&mut self) {
        SYNC_CALLS.lock().unwrap().remove(&self.0);
    }
}

/// Private adapter lookup; the C ABI and dispatch payload stay unchanged.
#[napi(js_name = "_getHostCallOrigin")]
pub fn get_host_call_origin(call_id: u32) -> Option<String> {
    sys_native::host_dispatch::origin_call_id(call_id).map(|origin| origin.0.to_string())
}

/// The JS queue/Promise owns this lease, rather than the BAML waiter. Explicit
/// exit releases it promptly; a discarded JS queue item can also release it.
#[derive(Clone)]
pub struct HostExecutionLease(Arc<Mutex<HostExecutionState>>);

struct HostExecutionState {
    call_id: u32,
    args: Option<Vec<u8>>,
    frame: Option<bridge_ctypes::OwnedHostInvocation>,
    execution: Option<sys_native::host_dispatch::HostExecutionGuard>,
}

impl HostExecutionLease {
    fn finish(&self) {
        let (args, frame, execution) = {
            let mut state = self.0.lock().unwrap();
            (
                state.args.take(),
                state.frame.take(),
                state.execution.take(),
            )
        };
        if let Some(args) = args {
            discard_host_call_args(&args);
        }
        drop((frame, execution));
    }
}

impl Drop for HostExecutionState {
    fn drop(&mut self) {
        if let Some(args) = self.args.take() {
            discard_host_call_args(&args);
        }
        // The execution guard drops after argument disposal.
    }
}

/// Claim actual JS execution atomically against waiter retirement.
#[napi(
    js_name = "_startHostCallExecution",
    ts_args_type = "execution: object"
)]
pub fn start_host_call_execution(execution: &External<HostExecutionLease>) -> Option<String> {
    let mut state = execution.0.lock().unwrap();
    let origin = sys_native::host_dispatch::start_execution(state.call_id)?;
    state.args = None; // Transfer argument ownership to JS decoding.
    Some(origin.0.to_string())
}

#[napi(js_name = "_hostInvocationFrame", ts_args_type = "execution: object")]
pub fn host_invocation_frame(
    execution: &External<HostExecutionLease>,
) -> napi::Result<(crate::handle::BamlHandle, Buffer)> {
    let state = execution.0.lock().unwrap();
    let frame = state
        .frame
        .as_ref()
        .ok_or_else(|| napi::Error::from_reason("callback already exited"))?;
    let key = crate::handle::handle_clone(frame.0.effective_state, "callback invocation capture")?;
    let invocation =
        crate::handle::BamlHandle::from_parts(key, BamlHandleType::InvocationState as i32);
    let cancel = bridge_cffi::control_projection::clone_outbound(
        frame
            .0
            .cancel
            .as_ref()
            .ok_or_else(|| napi::Error::from_reason("callback has no effective token"))?,
    )
    .map_err(crate::errors::bridge_error_to_napi)?;
    Ok((invocation, cancel.encode_to_vec().into()))
}

/// Close execution ownership only when the callback/Promise actually exits.
#[napi(
    js_name = "_finishHostCallExecution",
    ts_args_type = "execution: object"
)]
pub fn finish_host_call_execution(execution: &External<HostExecutionLease>) {
    execution.finish();
}

/// Release wire owners of a queued callback cancelled before JS execution.
/// Borrowed host keys belong to HostValueArc, not the native handle table.
fn discard_host_call_args(args: &[u8]) {
    if let Ok(call) = BamlToHostCall::decode(args) {
        for arg in call.args {
            if let Some(value) = arg.value {
                bridge_ctypes::release_outbound_references(value);
            }
        }
    }
    bex_project::host_release_dispatch::drain();
}

#[napi(js_name = "_discardHostCallArgs")]
pub fn discard_host_call_args_from_js(args: Buffer) {
    discard_host_call_args(&args);
}

/// Args the Rust-side dispatch forwards to the JS dispatch wrapper.
///
/// Wrapped in `FnArgs` because napi-rs's `JsValuesTupleIntoVec` is impl'd
/// on `FnArgs<(...)>`, not raw tuples.
pub(crate) type DispatchArgs = FnArgs<(u32, Buffer, External<HostExecutionLease>)>;

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

/// A JS callable handed to BAML: its dispatch wrapper and the function type
/// of the slot it was bound to.
struct Registered {
    tsfn: Arc<DispatchTsfn>,
    /// The result and thrown value are encoded against this. `None` for a
    /// callable bound under an open slot.
    signature: Option<bex_project::WireTy>,
}

/// Process-wide table of JS dispatch wrappers handed to BAML.
///
/// The key is a freshly-allocated `u64` (never 0). Removal happens in
/// [`host_release_callback`] when Rust drops its last clone of the
/// corresponding `HostValueArc`.
static REGISTRY: LazyLock<bridge_ctypes::HostValueRegistry<Registered>> =
    LazyLock::new(bridge_ctypes::HostValueRegistry::default);

/// The signature of each host call in flight, by callback id, from dispatch
/// until its result or error is encoded.
static CALLBACK_SIGNATURES: LazyLock<Mutex<HashMap<u32, Option<bex_project::WireTy>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn take_callback_signature(call_id: u32) -> Option<bex_project::WireTy> {
    CALLBACK_SIGNATURES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .remove(&call_id)
        .flatten()
}

/// Register a JS dispatch wrapper in the host-value table, bound to
/// `signature`, and return its key. `marker_key` is the host marker's handle
/// when the user function was defined through the instrumentation surface.
///
/// The `Function` is converted into a `ThreadsafeFunction` so it can outlive
/// the napi call scope and be invoked from any thread (the engine's tokio
/// runtime calls into this entry point from a worker thread).
pub(crate) fn register_callable(
    callable: Function<'_, DispatchArgs, ()>,
    marker_key: Option<u64>,
    signature: bex_project::WireTy,
) -> napi::Result<HostKey> {
    let descriptor = bridge_cffi::host_instrumentation::registration_marker(
        marker_key.unwrap_or(0),
        "typescript",
    )
    .map_err(crate::errors::bridge_error_to_napi)?;
    let tsfn: DispatchTsfn = callable
        .build_threadsafe_function()
        .callee_handled::<false>()
        .weak::<false>()
        // Bound the queue (napi's default is unbounded); see DISPATCH_QUEUE_SIZE.
        .max_queue_size::<DISPATCH_QUEUE_SIZE>()
        .build()?;
    let key = REGISTRY.insert(Registered {
        tsfn: Arc::new(tsfn),
        signature: Some(signature),
    });
    bex_project::HostValueArc::register_annotation(key, descriptor);
    Ok(key)
}

/// Complete an in-flight host call with its JavaScript result, encoded
/// against the return type of the slot the callable was bound to.
///
/// Exposed to JS as `completeHostCallValue(callId, value)`. A value that does
/// not fit completes the call as a `HostContractViolation` panic; a value the
/// bridge could not carry as an `SdkPanic`; an error the callable's own code
/// raised while the value was read is raised here, and the JS dispatch
/// wrapper then completes the call with that error.
#[napi(js_name = "completeHostCallValue")]
pub fn complete_host_call_value(
    env: Env,
    call_id: u32,
    value: Unknown<'_>,
) -> napi::Result<String> {
    let ret = match take_callback_signature(call_id) {
        Some(bex_project::WireTy::Function { ret, .. }) => *ret,
        Some(_) | None => bex_project::WireTy::Unknown,
    };
    // The outcome the engine sees, for the caller's instrumentation: a value
    // that completed the call as a panic is an error in the trace, whatever
    // the callable returned.
    let completion = crate::encode::encode_result(&env, value, &ret);
    let outcome = match &completion {
        Completion::Wire(_) => "ok",
        Completion::HostError(_)
        | Completion::ContractViolation(_)
        | Completion::BridgeFailure(_) => "error",
    };
    let wire = completion.into_wire(Encoded::Returned)?;
    complete(call_id, &wire);
    Ok(outcome.to_string())
}

/// Complete an in-flight host call with a thrown BAML value (the `value` of
/// a `BamlError`), encoded against the throws type of the slot the callable
/// was bound to. A `baml.panics.*` value is a panic whatever the slot
/// declares.
///
/// Exposed to JS as `completeHostCallThrow(callId, value)`. An error raised
/// while the value was read is raised here; the JS wrapper then completes the
/// call with it as the host's own exception.
#[napi(js_name = "completeHostCallThrow")]
pub fn complete_host_call_throw(env: Env, call_id: u32, value: Unknown<'_>) -> napi::Result<()> {
    let throws = match take_callback_signature(call_id) {
        Some(bex_project::WireTy::Function { throws, .. }) => *throws,
        Some(_) | None => bex_project::WireTy::Unknown,
    };
    let wire = crate::encode::encode_result(&env, value, &throws).into_wire(Encoded::Thrown)?;
    complete(call_id, &wire);
    Ok(())
}

/// Complete an in-flight host call as the host's own exception: the engine
/// builds the `baml.errors.HostCallable` whose `_handle` is `handle`, the
/// JS-side registration of the error object.
///
/// Exposed to JS as `completeHostCallRaised(callId, className, message, stack, handle)`.
#[napi(js_name = "completeHostCallRaised")]
pub fn complete_host_call_raised(
    call_id: u32,
    class_name: String,
    message: String,
    stack: Option<String>,
    handle: HandleKey,
) -> napi::Result<()> {
    drop(take_callback_signature(call_id));
    let handle = HostKey::new(handle.to_u64()).ok_or_else(|| {
        napi::Error::new(
            Status::InvalidArg,
            "a raised error must be registered first",
        )
    })?;
    complete(
        call_id,
        &bridge_encode::raised(message, class_name, "nodejs", stack, handle),
    );
    Ok(())
}

/// Complete an in-flight host call as a failure of the bridge's own: the
/// engine surfaces it as `baml.panics.SdkPanic`, never as a catchable error.
///
/// Exposed to JS as `completeHostCallBridgeFailure(callId, message)`.
#[napi(js_name = "completeHostCallBridgeFailure")]
pub fn complete_host_call_bridge_failure(call_id: u32, message: String) {
    drop(take_callback_signature(call_id));
    complete(call_id, &bridge_encode::bridge_failure(message));
}

fn complete(call_id: u32, completion: &HostCallCompletion) {
    let bytes = completion.encode_to_vec();
    bridge_cffi::complete_host_call(call_id, bytes.as_ptr() as *const i8, bytes.len());
}

/// Remove and drop the registry entry for `host_value_key` (if present).
///
/// Dropping the `ThreadsafeFunction` releases the underlying napi reference
/// (and its strong `weak::<false>` libuv ref), allowing the user's JS
/// callable to become GC-eligible and unpinning the event loop. Shared by the
/// engine-driven release path ([`host_release_callback`]) and the walker's
/// rollback path (`release_registered`).
fn drop_registry_entry(host_value_key: HostKey) {
    bex_project::HostValueArc::discard_annotation(host_value_key);
    let popped: Option<Registered> = match REGISTRY.lock() {
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
    if let Some(key) = HostKey::new(host_value_key) {
        drop_registry_entry(key);
    }
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
    HandleKey::from_u64(REGISTRY.mint_key().get())
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
/// Note this is the inverse of `register_callable`'s dispatch tsfn,
/// which is `weak::<false>()` — that one pins the loop because a hung
/// host callback awaiting completion *must* keep the loop alive so the
/// JS callback can actually run.
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
/// When an encode registers a callable for an early argument and a later
/// argument fails, the request is never sent, so the engine never decodes
/// (and so never releases) that key. Without this, the registry entry — and
/// its strong `weak::<false>` tsfn ref, which keeps the libuv loop alive —
/// would leak for the life of the process.
pub(crate) fn release_registered(key: HostKey) {
    drop_registry_entry(key);
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
pub extern "C" fn host_dispatch_callback(request: *const u8, length: usize) {
    if request.is_null() || length > isize::MAX as usize {
        return;
    }
    // SAFETY: V2 borrows runtime-owned envelope bytes for the callback.
    let bytes = unsafe { std::slice::from_raw_parts(request, length) };
    let frame = match bridge_cffi::invocation_protocol::decode_host_invocation(bytes) {
        Ok(frame) => bridge_ctypes::OwnedHostInvocation(frame),
        Err(_) => return,
    };
    let host_value_key = frame.0.host_value_key;
    let call_id = frame.0.callback_id;
    let application_args = frame.0.application_args.clone();
    dispatch_host_callable(
        host_value_key,
        call_id,
        application_args.as_ptr(),
        application_args.len(),
        frame,
    );
}

fn dispatch_host_callable(
    host_value_key: u64,
    call_id: u32,
    args: *const u8,
    length: usize,
    frame: bridge_ctypes::OwnedHostInvocation,
) {
    let Some(origin) = sys_native::host_dispatch::origin_call_id(call_id) else {
        // args owns wire handle references even after the waiter disappears.
        if !args.is_null() {
            // SAFETY: the same engine-owned slice contract as the copy below.
            discard_host_call_args(unsafe { std::slice::from_raw_parts(args, length) });
        }
        sys_native::host_dispatch::finish_abi_execution(call_id);
        return;
    };
    let is_sync = SYNC_CALLS.lock().unwrap().contains(&origin.0);
    if is_sync {
        if !args.is_null() {
            // SAFETY: args is valid for this callback's duration.
            discard_host_call_args(unsafe { std::slice::from_raw_parts(args, length) });
        }
        sys_native::host_dispatch::complete_with_error(
            call_id,
            OpError::new(
                SysOp::BamlHostCallHostValue,
                VmInternalError::BridgeFailure {
                    message: "host callables are only supported on the async call path; \
                              use the async API instead of synchronously invoking a \
                              returned closure that retains a host callback"
                        .to_string(),
                },
            ),
        );
        return;
    }
    let Some(execution) = sys_native::host_dispatch::retain_execution(call_id) else {
        if !args.is_null() {
            // SAFETY: args remains valid for the duration of dispatch.
            discard_host_call_args(unsafe { std::slice::from_raw_parts(args, length) });
        }
        sys_native::host_dispatch::finish_abi_execution(call_id);
        return;
    };

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
    // dispatch never blocks `register_callable` / `host_release_callback`.
    let tsfn: Option<Arc<DispatchTsfn>> = match REGISTRY.lock() {
        Ok(t) => HostKey::new(host_value_key)
            .and_then(|key| t.get(&key))
            .map(|entry| {
                // The result is encoded against the slot's signature once the
                // callable returns; keep it by callback id until then.
                CALLBACK_SIGNATURES
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .insert(call_id, entry.signature.clone());
                Arc::clone(&entry.tsfn)
            }),
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
        discard_host_call_args(&bytes);
        send_dispatch_error_no_callable(call_id, host_value_key);
        return;
    };

    // Schedule the JS dispatch wrapper. `NonBlocking` matches the Python
    // bridge's `Handle::spawn` semantics — control returns to the engine
    // promptly and the JS-side work happens on the libuv loop.
    let execution = HostExecutionLease(Arc::new(Mutex::new(HostExecutionState {
        call_id,
        args: Some(bytes.clone()),
        frame: Some(frame),
        execution: Some(execution),
    })));
    let status = tsfn.call(
        FnArgs::from((
            call_id,
            Buffer::from(bytes),
            External::new(execution.clone()),
        )),
        ThreadsafeFunctionCallMode::NonBlocking,
    );
    if status != Status::Ok {
        send_dispatch_error_tsfn_status(call_id, status);
        // napi may retain rejected queue data. Release shared owners explicitly.
        execution.finish();
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
    drop(take_callback_signature(call_id));
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
    drop(take_callback_signature(call_id));
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

#[napi(
    js_name = "_hostCaptureRequested",
    ts_args_type = "execution: object, outcome: string"
)]
pub fn host_capture_requested(execution: &External<HostExecutionLease>, outcome: String) -> bool {
    let id = execution.0.lock().unwrap().call_id;
    let outcome = if outcome == "ok" {
        btel_types::InvocationOutcome::Ok
    } else {
        btel_types::InvocationOutcome::Errored
    };
    bridge_cffi::host_instrumentation::callback(id).is_some_and(|owner| owner.wants_value(outcome))
}

#[napi(
    js_name = "_recordHostCallResult",
    ts_args_type = "execution: object, outcome: string, value?: string"
)]
pub fn record_host_call_result(
    execution: &External<HostExecutionLease>,
    outcome: String,
    value: Option<String>,
) {
    let id = execution.0.lock().unwrap().call_id;
    let outcome = match outcome.as_str() {
        "ok" => btel_types::InvocationOutcome::Ok,
        "cancelled" => btel_types::InvocationOutcome::Cancelled,
        _ => btel_types::InvocationOutcome::Errored,
    };
    if let Some(owner) = bridge_cffi::host_instrumentation::callback(id) {
        let value = value
            .filter(|_| owner.wants_value(outcome))
            .map(|value| crate::host_instrumentation::capture(&value));
        owner.observe(outcome, value);
    }
}
