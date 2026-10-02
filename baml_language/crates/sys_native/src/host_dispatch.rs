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
//!    waiter completion and independently retained execution state. Inserted
//!    before dispatch; delivery and actual exit retire their respective owners.
//!
//! These live in `sys_native` (not `bridge_cffi`) so that the sysop
//! implementation in `sys_native` can use them without introducing a circular
//! dependency (`bridge_cffi` already depends on `sys_native`).
//!
//! ## Independent waiter and execution lifetimes
//!
//! The completion handle belongs to the BAML waiter. Bridges claim a separate
//! execution lease before handing work to the host. Cancellation retires only
//! completion and requests cooperative host cancellation; the lease retains
//! invocation state and argument owners until the adapter reports actual exit.
//! A late completion is disposed without restoring the waiter. An indefinitely
//! running host execution intentionally retains its lease until it exits.

use std::{
    collections::HashMap,
    sync::{
        RwLock,
        atomic::{AtomicU32, Ordering},
    },
};

use once_cell::sync::{Lazy, OnceCell};
use sys_types::{BexExternalValue, CallId, CompletionHandle, OpError, SysOp, VmInternalError};

/// C-compatible dispatch callback installed by the host bridge.
///
/// Called by `call_host_value` when BAML code invokes a `HostValue`. The
/// bridge decodes `args`, invokes the host callable, and resolves the
/// in-flight call via `complete_host_call`.
///
/// ## Contract (upheld by the bridges)
///
/// * **Deliver at most once, report exit once.** The host reports its outcome
///   on every exit path, including exceptions. Cancellation may already have
///   retired delivery. Bridges using execution leases must release them only
///   when host execution actually exits (or queued work is discarded).
/// * **Dispatch itself is fire-and-return.** A bridge must hand execution to a
///   host task/goroutine before returning from this C callback. That worker may
///   re-enter the runtime with a separate BAML call while the original engine
///   call awaits completion; executing the user's callable inline on this C
///   callback stack remains unsupported.
pub type HostDispatchFn =
    extern "C" fn(host_value_key: u64, call_id: u32, args: *const u8, length: usize);

static HOST_DISPATCH_FN: OnceCell<HostDispatchFn> = OnceCell::new();

pub type HostDispatchV2 = extern "C" fn(request: *const u8, length: usize);
pub type HostCancelFn = extern "C" fn(callback_id: u32);
static HOST_DISPATCH_V2: OnceCell<HostDispatchV2> = OnceCell::new();
static HOST_CANCEL_FN: OnceCell<HostCancelFn> = OnceCell::new();
pub fn set_dispatch_v2(callback: HostDispatchV2) {
    let _ = HOST_DISPATCH_V2.set(callback);
}
pub fn set_cancel_fn(callback: HostCancelFn) {
    let _ = HOST_CANCEL_FN.set(callback);
}
pub fn fire_dispatch_v2(request: &[u8]) -> bool {
    match HOST_DISPATCH_V2.get() {
        Some(callback) => {
            callback(request.as_ptr(), request.len());
            true
        }
        None => false,
    }
}

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HostExecutionPhase {
    Queued,
    Running,
    Exited,
}

struct InflightCall {
    completion: Option<CompletionHandle>,
    origin: CallId,
    on_cancel: Option<Box<dyn FnOnce() + Send + Sync>>,
    execution: Option<HostExecutionPhase>,
    abi_execution: bool,
    // Includes the owning invocation context and original argument/callable
    // owners. Wire host keys are borrowed and cannot keep those owners alive.
    resources: Option<Box<dyn Send + Sync>>,
}

static TABLE: Lazy<RwLock<HashMap<u32, InflightCall>>> = Lazy::new(|| RwLock::new(HashMap::new()));

static NEXT_CALL_ID: AtomicU32 = AtomicU32::new(1);

// A host execution that never exits intentionally retains state. Wrapping ID
// collisions must therefore check the complete live set, including executions
// whose BAML waiters have already retired.

/// Allocate a nonzero candidate ID. Insertion rejects collisions after wrap.
pub fn next_call_id() -> u32 {
    allocate_candidate(&NEXT_CALL_ID)
}

fn allocate_candidate(counter: &AtomicU32) -> u32 {
    counter
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| {
            Some(id.wrapping_add(1).max(1))
        })
        .expect("candidate update always succeeds")
}

/// Register a completion for a fresh ID. The complete live set includes host
/// executions that have outlived their waiters, so cancellation cannot permit
/// an ID to be reused while an old host execution still owns it.
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
    insert_for_call(call_id, CallId(0), completion)
}

/// Register the dispatch together with its originating SDK call.
#[must_use]
pub fn insert_for_call(call_id: u32, origin: CallId, completion: CompletionHandle) -> bool {
    insert_with_resources(call_id, origin, completion, Box::new(()))
}

/// Retain invocation context and argument owners before crossing the bridge.
#[must_use]
pub fn insert_with_resources(
    call_id: u32,
    origin: CallId,
    completion: CompletionHandle,
    resources: Box<dyn Send + Sync>,
) -> bool {
    let mut table = TABLE.write().unwrap();
    let collision = call_id == 0 || table.contains_key(&call_id);
    if collision {
        // Release builds strip the assert above, so guard at runtime too.
        drop(table);
        tracing::error!(
            "host-call id {call_id} collided with a live in-flight entry; \
             refusing to overwrite and failing the new call"
        );
        completion.complete(Err(OpError::new(
            SysOp::BamlHostCallHostValue,
            VmInternalError::BridgeFailure {
                message: format!("host-call id {call_id} collided with a live in-flight call"),
            },
        )));
        return false;
    }
    table.insert(
        call_id,
        InflightCall {
            completion: Some(completion),
            origin,
            on_cancel: None,
            execution: None,
            abi_execution: false,
            resources: Some(resources),
        },
    );
    true
}

/// Read the issuing SDK call while a dispatch is still in flight. Bridges use
/// this to select the call's execution environment, never the callable's.
pub fn origin_call_id(call_id: u32) -> Option<CallId> {
    TABLE
        .read()
        .unwrap()
        .get(&call_id)
        .filter(|call| call.completion.is_some())
        .map(|call| call.origin)
}

/// Claim execution ownership before scheduling onto a host queue. Dropping
/// this guard reports exit. Transfer the guard itself to the host adapter,
/// which must retain it through actual exit (or discard of queued work).
pub fn retain_execution(call_id: u32) -> Option<HostExecutionGuard> {
    let mut table = TABLE.write().unwrap();
    let call = table.get_mut(&call_id)?;
    if call.completion.is_none() || (call.execution.is_some() && !call.abi_execution) {
        return None;
    }
    call.execution = Some(HostExecutionPhase::Queued);
    call.abi_execution = false;
    Some(HostExecutionGuard(Some(call_id)))
}

/// C ABI adapters report exit through `complete_host_call`, rather than a
/// Rust guard. Pin their resources before dispatch even if the waiter retires.
pub fn retain_abi_execution(call_id: u32) {
    if let Some(call) = TABLE.write().unwrap().get_mut(&call_id) {
        call.execution = Some(HostExecutionPhase::Running);
        call.abi_execution = true;
    }
}

pub fn finish_abi_execution(call_id: u32) {
    let owned = TABLE
        .read()
        .unwrap()
        .get(&call_id)
        .is_some_and(|call| call.abi_execution);
    if owned {
        finish_execution(call_id);
    }
}

pub struct HostExecutionGuard(Option<u32>);

impl Drop for HostExecutionGuard {
    fn drop(&mut self) {
        if let Some(call_id) = self.0.take() {
            finish_execution(call_id);
        }
    }
}

/// Claim the queued execution exactly once before decoding/calling host code.
/// Cancellation before this point prevents application execution.
pub fn start_execution(call_id: u32) -> Option<CallId> {
    let mut table = TABLE.write().unwrap();
    let call = table.get_mut(&call_id)?;
    if call.completion.is_none() || call.execution != Some(HostExecutionPhase::Queued) {
        return None;
    }
    call.execution = Some(HostExecutionPhase::Running);
    Some(call.origin)
}

/// Ancestry remains available after the BAML waiter has retired.
pub fn execution_origin(call_id: u32) -> Option<CallId> {
    TABLE
        .read()
        .unwrap()
        .get(&call_id)
        .filter(|call| {
            matches!(
                call.execution,
                Some(HostExecutionPhase::Queued | HostExecutionPhase::Running)
            )
        })
        .map(|call| call.origin)
}

/// Report actual host exit, independently of delivering its result. Repeated
/// reports are rejected. Host owners are always released outside the lock.
pub fn finish_execution(call_id: u32) -> bool {
    let (resources, hook, removed, abandoned) = {
        let mut table = TABLE.write().unwrap();
        let Some(call) = table.get_mut(&call_id) else {
            return false;
        };
        let Some(phase @ (HostExecutionPhase::Queued | HostExecutionPhase::Running)) =
            call.execution
        else {
            return false;
        };
        call.execution = Some(HostExecutionPhase::Exited);
        let abandoned = if phase == HostExecutionPhase::Queued {
            call.completion.take()
        } else {
            None
        };
        let resources = call.resources.take();
        let hook = call.on_cancel.take();
        let removed = if call.completion.is_none() {
            table.remove(&call_id)
        } else {
            None
        };
        (resources, hook, removed, abandoned)
    };
    if let Some(completion) = abandoned {
        completion.complete(Err(OpError::new(
            SysOp::BamlHostCallHostValue,
            VmInternalError::BridgeFailure {
                message: "host callback was discarded before execution".into(),
            },
        )));
    }
    drop((resources, hook, removed));
    bex_external_types::host_release_dispatch::drain();
    true
}

/// Attach a cooperative cancellation request to this dispatch.
/// Returns false if its waiter has already completed or been cancelled.
/// The hook runs once on guard eviction, outside the table lock; successful
/// completion removes it without invoking it. No C ABI changes are required.
pub fn set_cancellation_hook(call_id: u32, hook: impl FnOnce() + Send + Sync + 'static) -> bool {
    let mut hook: Option<Box<dyn FnOnce() + Send + Sync>> = Some(Box::new(hook));
    let installed = {
        let mut table = TABLE.write().unwrap();
        if let Some(call) = table
            .get_mut(&call_id)
            .filter(|call| call.completion.is_some())
        {
            std::mem::swap(&mut call.on_cancel, &mut hook);
            true
        } else {
            false
        }
    };
    // Dropping a hook may release host owners. Never do it under the lock.
    drop(hook);
    installed
}

/// Remove and return the `CompletionHandle` for the given call id, if any.
///
/// Returns `None` if no entry is present — the benign case hit by a stale
/// completion racing a cancellation, or by [`InflightGuard`]'s drop after the
/// call already completed normally.
pub fn take(call_id: u32) -> Option<CompletionHandle> {
    let (completion, hook, removed) = {
        let mut table = TABLE.write().unwrap();
        let call = table.get_mut(&call_id)?;
        let completion = call.completion.take();
        let hook = call.on_cancel.take();
        let removed = if matches!(call.execution, None | Some(HostExecutionPhase::Exited)) {
            table.remove(&call_id)
        } else {
            None
        };
        (completion, hook, removed)
    };
    drop((hook, removed));
    completion
}

/// Waiter ownership held by the sysop's async future. Drop retires result
/// delivery and requests host cancellation outside the lock. It does not
/// release a claimed execution lease or prove that host code has exited.
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
        let (completion, hook, removed) = {
            let mut table = TABLE.write().unwrap();
            let Some(call) = table.get_mut(&self.call_id) else {
                return;
            };
            let completion = call.completion.take();
            let hook = if completion.is_some() {
                call.on_cancel.take()
            } else {
                None
            };
            let removed = if matches!(call.execution, None | Some(HostExecutionPhase::Exited)) {
                table.remove(&self.call_id)
            } else {
                None
            };
            (completion, hook, removed)
        };
        // Only the delivery path ends here. A running host execution remains
        // independently owned until its actual exit, including during cleanup.
        let cancelled = completion.is_some();
        drop((completion, removed));
        if cancelled {
            if let Some(callback) = HOST_CANCEL_FN.get() {
                callback(self.call_id);
            }
        }
        if let Some(on_cancel) = hook {
            on_cancel();
        }
    }
}

/// Complete an in-flight call with a successful value.
pub fn complete_with_value(call_id: u32, value: BexExternalValue) {
    if let Some(c) = take(call_id) {
        c.complete(Ok(value));
    } else {
        tracing::debug!("complete_host_call for unknown call id {call_id}");
    }
    finish_abi_execution(call_id);
}

/// Complete an in-flight call with an error.
pub fn complete_with_error(call_id: u32, err: OpError) {
    if let Some(c) = take(call_id) {
        c.complete(Err(err));
    } else {
        tracing::debug!("complete_host_call(error) for unknown call id {call_id}");
    }
    finish_abi_execution(call_id);
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
        tracing::debug!("complete_host_call(throw) for unknown call id {call_id}");
    }
    finish_abi_execution(call_id);
}

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, sync::Arc};

    use sys_types::{SysOp, SysOpResult};

    use super::*;

    #[test]
    fn candidate_ids_wrap_without_returning_zero_or_exhausting_allocator() {
        let counter = AtomicU32::new(u32::MAX);
        assert_eq!(allocate_candidate(&counter), u32::MAX);
        assert_eq!(allocate_candidate(&counter), 1);
        assert_eq!(allocate_candidate(&counter), 2);
    }

    /// Test-only presence check that does not remove the entry.
    fn contains(call_id: u32) -> bool {
        TABLE.read().unwrap().contains_key(&call_id)
    }

    struct ExitProbe(Arc<std::sync::atomic::AtomicUsize>);

    impl Drop for ExitProbe {
        fn drop(&mut self) {
            // Host resource destruction may re-enter dispatch lookup.
            let _ = TABLE.read().unwrap().len();
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn cancelled_waiter_retains_execution_resources_and_ancestry_until_actual_exit() {
        let (_result, completion) = SysOpResult::pending(SysOp::BamlHostCallHostValue);
        let call_id = next_call_id();
        let origin = CallId(123);
        let exits = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        assert!(insert_with_resources(
            call_id,
            origin,
            completion,
            Box::new(ExitProbe(exits.clone()))
        ));
        let execution = retain_execution(call_id).unwrap();
        assert!(retain_execution(call_id).is_none());
        assert_eq!(start_execution(call_id), Some(origin));
        assert_eq!(start_execution(call_id), None);
        drop(InflightGuard::new(call_id));
        assert_eq!(origin_call_id(call_id), None);
        assert_eq!(execution_origin(call_id), Some(origin));
        assert_eq!(exits.load(Ordering::SeqCst), 0);
        // A late value is disposed and cannot recreate completion or end execution.
        complete_with_value(call_id, BexExternalValue::Int(7));
        assert!(take(call_id).is_none());
        assert_eq!(exits.load(Ordering::SeqCst), 0);
        drop(execution);
        assert_eq!(exits.load(Ordering::SeqCst), 1);
        assert_eq!(execution_origin(call_id), None);
        assert!(!finish_execution(call_id));
        assert!(!contains(call_id));
    }

    #[tokio::test]
    async fn dropping_queued_execution_fails_waiter_instead_of_stranding_it() {
        let (result, completion) = SysOpResult::pending(SysOp::BamlHostCallHostValue);
        let call_id = next_call_id();
        assert!(insert(call_id, completion));
        let execution = retain_execution(call_id).unwrap();
        drop(execution); // A host queue discarded the callback before starting it.
        assert!(!contains(call_id));
        let SysOpResult::Async(result) = result else {
            panic!("expected pending waiter");
        };
        let error = result.await.expect_err("discard must resolve the waiter");
        assert!(matches!(
            error.payload,
            sys_types::OpErrorPayload::Vm(sys_types::VmRustFnError::InternalError(
                VmInternalError::BridgeFailure { .. }
            ))
        ));
    }

    #[test]
    fn cancellation_before_host_start_disposes_queued_execution_without_running_it() {
        let (_result, completion) = SysOpResult::pending(SysOp::BamlHostCallHostValue);
        let call_id = next_call_id();
        assert!(insert(call_id, completion));
        let execution = retain_execution(call_id).unwrap();
        drop(InflightGuard::new(call_id));
        assert_eq!(start_execution(call_id), None);
        assert!(execution_origin(call_id).is_some());
        drop(execution);
        assert!(!contains(call_id));
    }

    #[test]
    fn result_delivery_and_actual_host_exit_can_happen_in_either_order() {
        for exit_first in [false, true] {
            let (_result, completion) = SysOpResult::pending(SysOp::BamlHostCallHostValue);
            let call_id = next_call_id();
            let exits = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            assert!(insert_with_resources(
                call_id,
                CallId(1),
                completion,
                Box::new(ExitProbe(exits.clone()))
            ));
            let _execution = retain_execution(call_id).unwrap();
            assert!(start_execution(call_id).is_some());
            if exit_first {
                assert!(finish_execution(call_id));
                assert!(origin_call_id(call_id).is_some());
                assert!(execution_origin(call_id).is_none());
                assert!(retain_execution(call_id).is_none());
                assert!(!finish_execution(call_id));
            }
            complete_with_error(
                call_id,
                OpError::new(
                    SysOp::BamlHostCallHostValue,
                    VmInternalError::BridgeFailure {
                        message: "host error".into(),
                    },
                ),
            );
            drop(InflightGuard::new(call_id));
            if !exit_first {
                assert_eq!(exits.load(Ordering::SeqCst), 0);
                assert!(finish_execution(call_id));
            }
            assert_eq!(exits.load(Ordering::SeqCst), 1);
            assert!(!finish_execution(call_id));
            assert!(!contains(call_id));
        }
    }

    #[test]
    fn cancellation_racing_actual_exit_releases_execution_once() {
        for _ in 0..64 {
            let (_result, completion) = SysOpResult::pending(SysOp::BamlHostCallHostValue);
            let call_id = next_call_id();
            let exits = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            assert!(insert_with_resources(
                call_id,
                CallId(1),
                completion,
                Box::new(ExitProbe(exits.clone()))
            ));
            let _execution = retain_execution(call_id).unwrap();
            assert!(start_execution(call_id).is_some());
            let barrier = Arc::new(std::sync::Barrier::new(2));
            let other = barrier.clone();
            let worker = std::thread::spawn(move || {
                other.wait();
                drop(InflightGuard::new(call_id));
            });
            barrier.wait();
            assert!(finish_execution(call_id));
            worker.join().unwrap();
            assert_eq!(exits.load(Ordering::SeqCst), 1);
            assert!(!finish_execution(call_id));
            assert!(!contains(call_id));
        }
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

    #[test]
    fn origin_is_scoped_to_one_dispatch_and_evicted_on_cancel() {
        let (_result, completion) = SysOpResult::pending(SysOp::BamlHostCallHostValue);
        let call_id = next_call_id();
        let origin = CallId(9876);
        assert!(insert_for_call(call_id, origin, completion));
        assert_eq!(origin_call_id(call_id), Some(origin));
        drop(InflightGuard::new(call_id));
        assert_eq!(origin_call_id(call_id), None);
    }

    #[test]
    fn cancellation_hook_runs_once_after_eviction_without_table_lock() {
        let (_result, completion) = SysOpResult::pending(SysOp::BamlHostCallHostValue);
        let call_id = next_call_id();
        assert!(insert_for_call(call_id, CallId(42), completion));
        let cancelled = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let observed = Arc::clone(&cancelled);
        assert!(set_cancellation_hook(call_id, move || {
            assert_eq!(origin_call_id(call_id), None);
            observed.fetch_add(1, Ordering::SeqCst);
        }));
        drop(InflightGuard::new(call_id));
        drop(InflightGuard::new(call_id));
        assert_eq!(cancelled.load(Ordering::SeqCst), 1);
        assert!(!set_cancellation_hook(call_id, || panic!(
            "already evicted"
        )));
    }

    #[test]
    fn normal_completion_does_not_cancel_host_execution() {
        let (_result, completion) = SysOpResult::pending(SysOp::BamlHostCallHostValue);
        let call_id = next_call_id();
        assert!(insert(call_id, completion));
        assert!(set_cancellation_hook(call_id, || panic!(
            "completed call cancelled"
        )));
        complete_with_value(call_id, BexExternalValue::Null);
        drop(InflightGuard::new(call_id));
        assert_eq!(origin_call_id(call_id), None);
    }

    #[test]
    fn completion_racing_cancellation_selects_one_terminal_owner() {
        for _ in 0..64 {
            let (_result, completion) = SysOpResult::pending(SysOp::BamlHostCallHostValue);
            let call_id = next_call_id();
            assert!(insert(call_id, completion));
            let cancelled = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let observed = Arc::clone(&cancelled);
            assert!(set_cancellation_hook(call_id, move || {
                observed.fetch_add(1, Ordering::SeqCst);
            }));
            let barrier = Arc::new(std::sync::Barrier::new(2));
            let cancel_barrier = Arc::clone(&barrier);
            let worker = std::thread::spawn(move || {
                cancel_barrier.wait();
                drop(InflightGuard::new(call_id));
            });
            barrier.wait();
            let completed = take(call_id).is_some();
            worker.join().unwrap();
            assert_eq!(cancelled.load(Ordering::SeqCst), usize::from(!completed));
            assert_eq!(origin_call_id(call_id), None);
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
            sys_types::OpErrorPayload::Vm(sys_types::VmRustFnError::InternalError(
                VmInternalError::BridgeFailure { message: _ }
            ))
        ));
    }
}
