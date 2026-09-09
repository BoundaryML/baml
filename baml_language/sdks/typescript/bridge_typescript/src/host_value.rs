//! Per-process Node host-value registration and callback dispatch.
//!
//! Ordinary JS callables register a wrapper that decodes an owned
//! `BamlToHostCall`, invokes the user body, and encodes its completion.
//! The native dispatch queue owns arguments through scheduling and releases
//! undelivered messages on queue failure or environment shutdown. JS decoding
//! adopts all arguments before invoking user code; retained arguments then
//! have ordinary SDK ownership independent of this call's completion.
//!
//! The registry strongly retains each callable while engine/SDK ownership
//! exists. Final HostValueArc release removes its dispatch queue, allowing
//! Node to release the JS callable reference. Collection and release-drain
//! timing remain part of the common runtime lifetime work; this is not a
//! permanent cache of every callback ever passed to the singleton.

use std::{
    collections::HashMap,
    sync::{
        Arc, LazyLock, Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};

use napi::{
    Status,
    bindgen_prelude::{Buffer, Function},
};
use napi_derive::napi;
use sys_native::{OpError, SysOp, VmInternalError};

use crate::{encoded_result::BamlEncodedResult, handle::HandleKey};
use bridge_ctypes::{HANDLE_TABLE, TransferSession};

use crate::dispatch_queue::{DispatchArgs, DispatchQueue};
use crate::release_queue::{ReleaseQueue, ReleaseQueueStats};
type DispatchTsfn = DispatchQueue;

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
    // `DispatchQueue` does not implement `Clone`, so we wrap in `Arc`
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
/// The `Function` is retained by an owned dispatch queue so it can outlive
/// the napi call scope and be invoked from any thread (the engine's tokio
/// runtime calls into this entry point from a worker thread).
#[napi(ts_args_type = "callable: (callId: number, args: BamlEncodedResult) => void")]
pub fn register_host_callable(callable: Function<'_, DispatchArgs, ()>) -> napi::Result<HandleKey> {
    let tsfn = DispatchQueue::new(callable, DISPATCH_QUEUE_SIZE)?;
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
/// Dropping the dispatch queue releases the underlying napi reference
/// allowing the user's JS callable to become GC-eligible. Callable ownership
/// does not itself pin the Node event loop. Shared by the
/// engine-driven release path ([`host_release_callback`]) and the encoder's
/// rollback path ([`release_host_callable`]).
fn drop_registry_entry(host_value_key: u64) {
    // The map contains primitive keys and Arc values, with no user code under
    // this lock. Rust's HashMap remains valid after unwind; recover poison and
    // remove the owner rather than deliberately leaking it. Drop outside it.
    let popped = REGISTRY
        .table
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .remove(&host_value_key);
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
    if let Some(queue) = HOST_VALUE_RELEASE_CALLBACK.get() {
        let status = queue.enqueue(host_value_key);
        if status != Status::Ok && status != Status::Closing {
            log::error!("host-release wakeup failed: {status:?}; release key retained");
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
// Pending keys live in a coalesced native set until JS acknowledges deletion.
// At most one wakeup is queued; the channel cannot overflow a per-key queue.
// Empty batches release their storage instead of retaining a high-water cache.
static HOST_VALUE_RELEASE_CALLBACK: OnceLock<Arc<ReleaseQueue>> = OnceLock::new();

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

/// Install the SDK's idempotent batch-deletion callback. First registration
/// wins. The channel is unreferenced so it does not keep Node running; closing
/// an environment also releases any native pending-key storage.
#[napi(ts_args_type = "callback: (keys: Array<HandleKey>) => void")]
pub fn register_host_value_release_callback(
    callback: Function<'_, Vec<HandleKey>, ()>,
) -> napi::Result<()> {
    let queue = ReleaseQueue::new(callback)?;
    let _ = HOST_VALUE_RELEASE_CALLBACK.set(Arc::new(queue));
    Ok(())
}

#[napi(js_name = "_hostReleaseStats")]
pub fn host_release_stats() -> napi::Result<ReleaseQueueStats> {
    HOST_VALUE_RELEASE_CALLBACK
        .get()
        .map(|queue| queue.stats())
        .ok_or_else(|| napi::Error::from_reason("Host release callback is not installed"))
}

/// Drive real final table-owner releases in a single JS turn for burst tests.
#[napi(js_name = "_releaseHostReferencesForTest")]
pub fn release_host_references_for_test(keys: Vec<HandleKey>) -> napi::Result<()> {
    for key in keys {
        let host = bex_project::HostValueArc::new(key.to_u64(), bex_project::HostValueKind::Opaque);
        let lease = HANDLE_TABLE.insert(bridge_ctypes::CffiHandleTableEntry::HostValue(host));
        bridge_cffi::handle::release_handle(lease)
            .map_err(|error| napi::Error::from_reason(error.to_string()))?;
    }
    Ok(())
}

/// Release a host callable the inbound encoder registered but never handed to
/// the engine — the encode-error rollback path.
///
/// Exposed to JS as `releaseHostCallable(key)`. When `encodeCallArgs`
/// registers a callable for an early kwarg and then fails to encode a later
/// kwarg, the `CallFunctionArgs` is never sent, so the engine never decodes
/// (and so never releases) that key. Without this, the registry entry — and
/// its retained JavaScript function — would leak for the life of the process. The encoder calls this for every
/// key it registered during a failed encode.
#[napi(js_name = "releaseHostCallable")]
pub fn release_host_callable(key: HandleKey) {
    drop_registry_entry(key.to_u64());
}

/// Dispatch an owned argument aggregate to JavaScript. A failed lookup or
/// schedule drops it without publishing any argument handles.
pub fn host_dispatch_callback(
    host_value_key: u64,
    call_id: u32,
    delivery: sys_native::host_dispatch::OwnedHostCall,
) {
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

    // This delivery owns its own receipt, independent of the issuing call's
    // cancellation. A queued dispatch may still need to decode/release after
    // cancellation; adopted arguments may be retained by the host callback.
    // Never attach it to a potentially replaced process-global runtime here.
    let args = (|| {
        let (runtime, invocation_session) =
            bridge_cffi::RuntimeIssuer::from_host_context(delivery.issuer.as_ref())
                .map_err(|error| napi::Error::from_reason(error.to_string()))?;
        BamlEncodedResult::with_invocation_session(
            delivery.arguments,
            TransferSession::new(&HANDLE_TABLE),
            Some(runtime),
            invocation_session,
        )
    })();
    let args = match args {
        Ok(args) => args,
        Err(error) => {
            sys_native::host_dispatch::complete_with_error(
                call_id,
                OpError::new(
                    SysOp::BamlHostCallHostValue,
                    VmInternalError::BridgeFailure {
                        message: format!("failed to stage host arguments: {error}"),
                    },
                ),
            );
            return;
        }
    };

    // Schedule the JS dispatch wrapper. `NonBlocking` matches the Python
    // bridge's blocking-task dispatch — control returns to the engine
    // promptly and the JS-side work happens on the libuv loop.
    let status = tsfn.call(call_id, args);
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

pub(crate) fn send_dispatch_bridge_failure(call_id: u32, message: String) {
    sys_native::host_dispatch::complete_with_error(
        call_id,
        OpError::new(
            SysOp::BamlHostCallHostValue,
            VmInternalError::BridgeFailure { message },
        ),
    );
}
