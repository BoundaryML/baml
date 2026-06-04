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
    collections::HashMap,
    sync::{
        Arc, LazyLock, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use bridge_ctypes::baml_core::cffi::{
    InboundClassValue, InboundMapEntry, InboundValue, inbound_map_entry::Key as InboundMapKey,
    inbound_value::Value as InboundValueVariant,
};
use napi::{
    Status,
    bindgen_prelude::{Buffer, FnArgs, Function},
    threadsafe_function::{ThreadsafeFunction, ThreadsafeFunctionCallMode},
};
use napi_derive::napi;
use prost::Message;

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
/// (and its strong `weak::<false>` libuv ref), allowing the user's JS
/// callable to become GC-eligible and unpinning the event loop. Shared by the
/// engine-driven release path ([`host_release_callback`]) and the encoder's
/// rollback path ([`release_host_callable`]).
fn drop_registry_entry(host_value_key: u64) {
    let popped: Option<Arc<DispatchTsfn>> = match REGISTRY.table.lock() {
        Ok(mut t) => t.remove(&host_value_key),
        Err(_) => return, // poisoned; nothing we can do safely
    };
    drop(popped);
}

/// Drop the JS dispatch wrapper associated with `host_value_key`.
///
/// Fires when the last Rust clone of the corresponding `HostValueArc` is
/// dropped — see `bex_external_types::host_value::host_release_dispatch`.
pub extern "C" fn host_release_callback(host_value_key: u64) {
    drop_registry_entry(host_value_key);
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
    let tsfn: Option<Arc<DispatchTsfn>> = match REGISTRY.table.lock() {
        Ok(t) => t.get(&host_value_key).cloned(),
        Err(_) => None,
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

/// Build an `InboundValue` carrying a `baml.errors.HostCallable` Instance.
/// Mirrors `bridge_python::host_value::build_host_callable_inbound`.
fn build_host_callable_inbound(class_name: &str, message: &str) -> InboundValue {
    fn string_field(key: &str, value: &str) -> InboundMapEntry {
        InboundMapEntry {
            key: Some(InboundMapKey::StringKey(key.to_string())),
            value: Some(InboundValue {
                value: Some(InboundValueVariant::StringValue(value.to_string())),
            }),
        }
    }
    InboundValue {
        value: Some(InboundValueVariant::ClassValue(InboundClassValue {
            name: "baml.errors.HostCallable".to_string(),
            fields: vec![
                string_field("message", message),
                string_field("class_name", class_name),
                string_field("language", "nodejs"),
            ],
        })),
    }
}

/// Synthesize a thrown `baml.errors.HostCallable` for "no registered JS
/// callable for this host-value key" and forward it via `complete_host_call`.
fn send_dispatch_error_no_callable(call_id: u32, host_value_key: u64) {
    let bytes = build_host_callable_inbound(
        "KeyError",
        &format!("no host callable registered for key {host_value_key}"),
    )
    .encode_to_vec();
    bridge_cffi::complete_host_call(call_id, 1, bytes.as_ptr() as *const i8, bytes.len());
}

/// Synthesize a thrown `baml.errors.HostCallable` for a tsfn-scheduling
/// failure (queue full / aborted / library shutdown) and forward it via
/// `complete_host_call`.
fn send_dispatch_error_tsfn_status(call_id: u32, status: Status) {
    let bytes = build_host_callable_inbound(
        "RuntimeError",
        &format!("threadsafe_function call failed with status {status:?}"),
    )
    .encode_to_vec();
    bridge_cffi::complete_host_call(call_id, 1, bytes.as_ptr() as *const i8, bytes.len());
}
