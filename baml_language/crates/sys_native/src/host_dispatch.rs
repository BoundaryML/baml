//! Host-value dispatch infrastructure for the `call_host_value` sysop.
//!
//! This module provides two shared services:
//!
//! 1. **Dispatch function pointer** (`HostDispatchFn`) — a process-global
//!    callback installed by the bridge (via `bridge_cffi`) when the host
//!    language registers its dispatch function. The `call_host_value` sysop
//!    reads it to invoke the host callable.
//!
//! 2. **Call table** — a process-global map from `call_id: u32` to
//!    `CompletionHandle`. Inserted by `call_host_value` before dispatch;
//!    removed and resolved by `complete_host_call` (also `bridge_cffi`).
//!
//! These live in `sys_native` (not `bridge_cffi`) so that the sysop
//! implementation in `sys_native` can use them without introducing a circular
//! dependency (`bridge_cffi` already depends on `sys_native`).
//!
//! ## In-flight lifetime: no wall-clock timeout
//!
//! There is **no per-call timeout**. An in-flight entry stays in the table
//! until one of two things happens:
//!
//! 1. The host calls `complete_host_call(call_id, ...)`, which `take`s the
//!    entry and fires its `CompletionHandle`. Normal completion.
//! 2. The BAML call that issued the host call is cancelled. The engine drops
//!    the sysop's async future (the `tokio::select!` cancel arm in
//!    `bex_engine`), which drops the [`InflightGuard`] moved into that future
//!    (constructed by the `host_impls` sysop impl); the guard's `Drop` `take`s
//!    the dangling entry, so cancellation evicts it — no leak.
//!
//! A host that *never* completes a call and is *never* cancelled leaves its
//! entry pending forever. There is currently no engine-side watchdog that
//! synthesizes a timeout error. Hung host code is the host's responsibility;
//! cancellation (the only eviction signal besides completion) is driven by the
//! caller's cancel token, not a clock.

use std::{
    collections::HashMap,
    sync::{
        RwLock,
        atomic::{AtomicU32, Ordering},
    },
};

use once_cell::sync::{Lazy, OnceCell};
use sys_types::{BexExternalValue, CompletionHandle, OpError, SysOp, VmBamlError};

/// C-compatible dispatch callback installed by the host bridge.
///
/// Called by `call_host_value` when BAML code invokes a `HostValue`. The
/// bridge decodes `args`, invokes the host callable, and resolves the
/// in-flight call via `complete_host_call`.
///
/// ## Contract (upheld by the bridges)
///
/// * **Complete exactly once.** Every dispatched `call_id` must be resolved by
///   exactly one `complete_host_call` (success or error), on every exit path —
///   including host-side exceptions and panics. There is no engine-side timeout
///   (see "In-flight lifetime" above), so a call that is never completed and
///   never cancelled hangs the issuing BAML call indefinitely.
/// * **Dispatch itself is fire-and-return.** A bridge must hand execution to a
///   host task/goroutine before returning from this C callback. That worker may
///   re-enter the runtime with a separate BAML call while the original engine
///   call awaits completion; executing the user's callable inline on this C
///   callback stack remains unsupported.
pub type HostDispatchFn =
    extern "C" fn(host_value_key: u64, call_id: u32, args: *const u8, length: usize);

static HOST_DISPATCH_FN: OnceCell<HostDispatchFn> = OnceCell::new();

/// Install the dispatch callback. First-call-wins; subsequent calls are
/// silently ignored (consistent with `register_callback` semantics).
pub fn set_dispatch_fn(f: HostDispatchFn) {
    let _ = HOST_DISPATCH_FN.set(f);
}

/// Invoke the registered dispatch callback.
///
/// Returns `true` if the callback was installed and fired, `false` if
/// no bridge has registered a dispatcher yet. The caller is responsible
/// for resolving the in-flight `CompletionHandle` on `false`.
pub fn fire_dispatch(host_value_key: u64, call_id: u32, args: &[u8]) -> bool {
    match HOST_DISPATCH_FN.get() {
        Some(f) => {
            // The dispatch fn is fire-and-return: every bridge hands the call
            // off to the host (spawning a task / goroutine / threadsafe-fn
            // callback) and returns promptly, then later resolves the in-flight
            // `CompletionHandle` via `complete_host_call`. It never blocks on
            // the host's response, so we call it directly. A
            // `tokio::task::block_in_place` wrapper would only be needed for a
            // blocking callee, and it would panic on a current-thread runtime —
            // neither applies here.
            f(host_value_key, call_id, args.as_ptr(), args.len());
            true
        }
        None => {
            tracing::warn!(
                "call_host_value invoked before register_host_dispatch_callback: \
                 no host dispatch fn registered"
            );
            false
        }
    }
}

// ============================================================================
// In-flight call table
// ============================================================================

static TABLE: Lazy<RwLock<HashMap<u32, CompletionHandle>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

static NEXT_CALL_ID: AtomicU32 = AtomicU32::new(1);

// NOTE: there is no leak detection for hung host callables. A host
// bridge that never calls `complete_host_call` for a given `call_id`
// (and whose BAML caller never cancels) leaves a permanent entry in
// `TABLE`. On a long-running process such entries could accumulate, and
// after enough inserts the `u32` `call_id` space would wrap — at which
// point [`insert`] catches the collision and fails the new call (it does
// NOT corrupt the dead entry), but every subsequent call with the same
// id would keep failing until the dead entry is somehow evicted.
//
// A size-based warning at `insert` time can't distinguish "legitimate
// burst of concurrent calls" from "slow accumulation of hung calls";
// the only signal that actually separates them is per-entry age. If
// this becomes a real concern, add a `created_at: Instant` to
// `CompletionHandle` and warn on any entry older than some threshold
// (~5min).

/// Allocate a fresh call id that is unique across the process lifetime.
///
/// Zero is reserved as "invalid"; wrap-around skips 0.
pub fn next_call_id() -> u32 {
    loop {
        let id = NEXT_CALL_ID.fetch_add(1, Ordering::Relaxed);
        if id != 0 {
            return id;
        }
    }
}

/// Register a `CompletionHandle` for an in-flight host call.
///
/// `call_id` must be freshly minted by [`next_call_id`] and not already
/// present in the table. The table is keyed by a *wrapping* `u32` (kept narrow
/// for the C ABI), so in principle a wrap-around could collide with a still
/// live entry. With RAII eviction of cancelled calls (see [`InflightGuard`])
/// entries no longer leak, so the live set stays bounded and a collision is
/// effectively impossible.
///
/// We never silently overwrite an existing entry — doing so would strand the
/// previous call's `CompletionHandle` (its future would hang forever) and let a
/// late completion resolve the wrong call. A collision trips a `debug_assert!`
/// in debug builds; in release builds (where the assert is stripped) it is
/// caught at runtime by refusing the insert and completing the new call with an
/// error.
///
/// Returns `true` when `completion` was inserted, `false` on collision. On
/// `false` the caller **must not** fire the host dispatch (the call has already
/// failed) and **must not** build an [`InflightGuard`] for `call_id` — the live
/// entry under that id belongs to the *other* call, so a guard drop would evict
/// it.
#[must_use]
pub fn insert(call_id: u32, completion: CompletionHandle) -> bool {
    let mut table = TABLE.write().unwrap();
    let collision = table.contains_key(&call_id);
    debug_assert!(
        !collision,
        "host-call id {call_id} collided with a live in-flight entry; the u32 \
         call-id space wrapped while an entry was still pending (this should be \
         impossible now that cancelled calls are evicted via InflightGuard)"
    );
    if collision {
        // Release builds strip the assert above, so guard at runtime too.
        drop(table);
        tracing::error!(
            "host-call id {call_id} collided with a live in-flight entry; \
             refusing to overwrite and failing the new call"
        );
        completion.complete(Err(OpError::new(
            SysOp::BamlHostCallHostValue,
            VmBamlError::DevOther {
                message: format!("host-call id {call_id} collided with a live in-flight call"),
            },
        )));
        return false;
    }
    table.insert(call_id, completion);
    true
}

/// Remove and return the `CompletionHandle` for the given call id, if any.
///
/// Returns `None` if no entry is present — the benign case hit by a stale
/// completion racing a cancellation, or by [`InflightGuard`]'s drop after the
/// call already completed normally.
pub fn take(call_id: u32) -> Option<CompletionHandle> {
    TABLE.write().unwrap().remove(&call_id)
}

/// RAII guard that evicts an in-flight call's table entry when dropped.
///
/// Owned by the sysop's async future (the `host_impls` `call_host_value` impl).
/// It carries only the `call_id` (a `Copy` `u32`), never the
/// `CompletionHandle` — that handle lives in the table and is owned by whoever
/// `take`s it. On drop the guard calls [`take`], which:
///
/// * **After normal completion** is a no-op: `complete_host_call` already
///   `take`-and-fired the handle, so the entry is gone and `take` returns
///   `None`.
/// * **On cancellation** removes the still-present entry. Dropping the removed
///   `CompletionHandle` closes the oneshot sender, so the host's later
///   `complete_host_call` for that id hits the benign unknown-id path. No leak.
pub struct InflightGuard {
    call_id: u32,
}

impl InflightGuard {
    /// Create a guard for an already-`insert`ed `call_id`.
    pub fn new(call_id: u32) -> Self {
        Self { call_id }
    }
}

impl Drop for InflightGuard {
    fn drop(&mut self) {
        // Evict the entry if it is still present (cancellation). After normal
        // completion the entry is already gone, so this is a no-op.
        let _ = take(self.call_id);
    }
}

/// Complete an in-flight call with a successful value.
pub fn complete_with_value(call_id: u32, value: BexExternalValue) {
    if let Some(c) = take(call_id) {
        c.complete(Ok(value));
    } else {
        tracing::warn!("complete_host_call for unknown call id {call_id}");
    }
}

/// Complete an in-flight call with an error.
pub fn complete_with_error(call_id: u32, err: OpError) {
    if let Some(c) = take(call_id) {
        c.complete(Err(err));
    } else {
        tracing::warn!("complete_host_call(error) for unknown call id {call_id}");
    }
}

/// Complete an in-flight host-callable call with a *thrown value* — the
/// host language invoked the callable and the callable raised the
/// decoded `BexExternalValue`. The engine will run the declared-throws
/// contract check against `value` and either inject it as a catchable
/// throw or as a `baml.panics.HostContractViolation` panic; see
/// `bex_engine`'s host-throw delivery path.
///
/// Unlike [`complete_with_error`] (which delivers an inherent error from
/// the bridge / infrastructure layer), this carries a host *throw* that
/// must be checked against `E` before it can become an unwind value.
pub fn complete_with_throw(call_id: u32, value: BexExternalValue) {
    if let Some(c) = take(call_id) {
        c.complete(Err(OpError::host_thrown_value(
            sys_types::SysOp::BamlHostCallHostValue,
            value,
        )));
    } else {
        tracing::warn!("complete_host_call(throw) for unknown call id {call_id}");
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use sys_types::{SysOp, SysOpResult};

    use super::*;

    /// Test-only presence check that does not remove the entry.
    fn contains(call_id: u32) -> bool {
        TABLE.read().unwrap().contains_key(&call_id)
    }

    /// Minted ids are unique, monotonic, and never 0 (reserved sentinel).
    #[test]
    fn next_call_id_is_unique_and_nonzero() {
        let mut seen = HashSet::new();
        for _ in 0..1000 {
            let id = next_call_id();
            assert_ne!(id, 0, "minted call id must never be the reserved 0");
            assert!(seen.insert(id), "minted call id {id} was handed out twice");
        }
    }

    /// Cancellation: the sysop future (carrying the [`InflightGuard`]) is
    /// dropped before completion → the guard evicts the in-flight entry, so it
    /// does not leak. Without the guard the entry would linger forever (the
    /// engine never learns the private `call_id`).
    #[test]
    fn guard_drop_evicts_in_flight_entry_on_cancel() {
        let (result, completion) = SysOpResult::pending(SysOp::BamlHostCallHostValue);
        let call_id = next_call_id();
        assert!(insert(call_id, completion), "fresh id must insert cleanly");
        assert!(contains(call_id), "entry must be present after insert");

        // Model the engine dropping the cancelled sysop future: the future owns
        // both the awaited oneshot receiver (`result`) and the guard.
        let guard = InflightGuard::new(call_id);
        let fut = async move {
            let _guard = guard;
            result
        };
        drop(fut);

        assert!(
            !contains(call_id),
            "guard drop on cancel must evict the in-flight entry (no leak)"
        );
    }

    /// Normal completion: `complete_with_value` already `take`s the entry, so a
    /// later guard drop is a benign no-op (no double-take problem, no panic).
    #[tokio::test]
    async fn guard_drop_after_normal_completion_is_noop() {
        let (result, completion) = SysOpResult::pending(SysOp::BamlHostCallHostValue);
        let call_id = next_call_id();
        assert!(insert(call_id, completion), "fresh id must insert cleanly");
        let guard = InflightGuard::new(call_id);

        // Host completes normally — this takes-and-fires the handle.
        complete_with_value(call_id, BexExternalValue::Int(7));
        assert!(
            !contains(call_id),
            "completion must remove the in-flight entry"
        );

        // The awaited value resolves as expected.
        let value = match result {
            SysOpResult::Async(fut) => fut.await.expect("expected Ok"),
            SysOpResult::Ready(Ok(v)) => v,
            SysOpResult::Ready(Err(e)) => panic!("unexpected error: {e}"),
        };
        assert!(matches!(value, BexExternalValue::Int(7)));

        // Guard drop now is a no-op: the entry is already gone.
        drop(guard);
        assert!(!contains(call_id));
    }

    /// Minting + inserting a fresh id never collides with a live entry: a fresh
    /// id is, by construction, not already present, so `insert`'s
    /// `debug_assert!` does not fire. (With RAII eviction in place, entries do
    /// not leak, so the live set stays bounded and a real collision is
    /// impossible.)
    #[test]
    fn fresh_id_insert_does_not_collide_with_live_entry() {
        // Stand up a batch of live entries.
        let mut live = Vec::new();
        for _ in 0..64 {
            let (_result, completion) = SysOpResult::pending(SysOp::BamlHostCallHostValue);
            let id = next_call_id();
            assert!(insert(id, completion), "fresh id must insert cleanly");
            live.push(id);
        }

        // A freshly minted id is not among the live set, so inserting it does
        // not trip the collision assert.
        let (_result, completion) = SysOpResult::pending(SysOp::BamlHostCallHostValue);
        let fresh = next_call_id();
        assert!(
            !live.contains(&fresh),
            "freshly minted id collided with a live entry"
        );
        // Would panic via the `debug_assert!` on collision; returns `true`
        // (inserted) for a fresh id.
        assert!(insert(fresh, completion), "fresh id must insert cleanly");

        // Clean up so the global table does not retain entries across tests.
        for id in live {
            let _ = take(id);
        }
        let _ = take(fresh);
    }

    /// In release builds the `debug_assert!` is stripped, so a (wrapped)
    /// collision is handled at runtime: `insert` returns `false`, leaves the
    /// existing live entry untouched, and completes the *new* handle with an
    /// error so its caller can bail without firing the host dispatch. This path
    /// is unreachable in debug builds (the assert fires first), hence the gate.
    #[cfg(not(debug_assertions))]
    #[tokio::test]
    async fn collision_insert_returns_false_and_preserves_live_entry() {
        let (first_result, first_completion) = SysOpResult::pending(SysOp::BamlHostCallHostValue);
        let call_id = next_call_id();
        assert!(
            insert(call_id, first_completion),
            "first insert must succeed"
        );

        // Second insert for the SAME id (models a u32 wrap onto a live entry).
        let (second_result, second_completion) = SysOpResult::pending(SysOp::BamlHostCallHostValue);
        assert!(
            !insert(call_id, second_completion),
            "collision insert must return false"
        );

        // The original live entry is untouched: completing `call_id` resolves
        // the FIRST call.
        complete_with_value(call_id, BexExternalValue::Int(1));
        let first = match first_result {
            SysOpResult::Async(fut) => fut.await.expect("first call resolves"),
            SysOpResult::Ready(Ok(v)) => v,
            SysOpResult::Ready(Err(e)) => panic!("unexpected error: {e}"),
        };
        assert!(matches!(first, BexExternalValue::Int(1)));

        // The second (rejected) call was already completed with an error by
        // `insert` itself.
        let second_err = match second_result {
            SysOpResult::Async(fut) => fut.await.expect_err("second call must error"),
            SysOpResult::Ready(Ok(_)) => panic!("expected an error for the rejected call"),
            SysOpResult::Ready(Err(e)) => e,
        };
        assert!(matches!(
            second_err.payload,
            sys_types::OpErrorPayload::Vm(sys_types::VmRustFnError::BamlError(
                VmBamlError::DevOther { message: _ }
            ))
        ));
    }
}
