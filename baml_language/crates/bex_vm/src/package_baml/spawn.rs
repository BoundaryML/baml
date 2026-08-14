//! Native implementations for `namespace baml.spawn` (BEP-034 spawn options):
//! the `CancelToken` and `TaskGroup` classes and the `options(...)` factory.
//!
//! These are opaque classes whose `_handle` field holds an
//! `Object::RustData(Arc<…>)`: a `CancellationToken` for `CancelToken`, an
//! `Arc<TaskGroupInner>` for `TaskGroup`, and a `SpawnConfigData` for
//! `SpawnConfig`. They are built with the standard `resolve_class` +
//! `alloc_rust_data` + `alloc_instance` pattern and read back via `as_instance`
//! + `load_field(0)` + `as_rust_data`.
#![allow(unsafe_code)]

use std::sync::Arc;

use bex_heap::TlabHolder;
use bex_vm_types::{
    TaskGroupInner,
    types::{CancellationToken, Value},
};

use super::{
    BamlClassSpawnCancelToken, BamlClassSpawnTaskGroup, BamlNamespaceSpawn, PackageBamlImpl, view,
};
use crate::BexVm;

/// Read the runtime `CancellationToken` out of a `baml.spawn.CancelToken`
/// receiver. Field 0 (`_handle`) is the `Object::RustData` wrapping the token.
/// Returns `None` if the value is not a well-formed `CancelToken` instance
/// (including the `OmittedArg` sentinel for an omitted optional argument).
fn as_cancellation_token(vm: &BexVm, value: Value) -> Option<CancellationToken> {
    let instance = vm.as_instance(&value).ok()?;
    let handle = instance.load_field(0);
    vm.as_rust_data::<CancellationToken>(&handle).ok().cloned()
}

/// Allocate a fresh `baml.spawn.CancelToken` instance wrapping `token`.
fn alloc_cancel_token(vm: &mut BexVm, token: CancellationToken) -> Value {
    let class = vm.resolve_class("baml.spawn.CancelToken");
    let handle = Value::object(vm.alloc_rust_data(Arc::new(token)));
    Value::object(vm.alloc_instance(class, vec![handle]))
}

/// Allocate a fresh `baml.spawn.TaskGroup` instance wrapping `inner`.
fn alloc_task_group(vm: &mut BexVm, inner: Arc<TaskGroupInner>) -> Value {
    let class = vm.resolve_class("baml.spawn.TaskGroup");
    let handle = Value::object(vm.alloc_rust_data(inner));
    Value::object(vm.alloc_instance(class, vec![handle]))
}

/// Convert a `usize` count/limit to `i64`, saturating at `i64::MAX`. Group
/// counts and limits are always small in practice; this just makes the cast
/// explicitly lossless-or-saturating for clippy.
fn clamp_to_i64(value: usize) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

#[allow(clippy::used_underscore_items)]
impl BamlClassSpawnCancelToken for PackageBamlImpl {
    // The generated trait fixes the return type as `Value` (the heap
    // `CancelToken` instance), so it cannot return `Self`.
    #[allow(clippy::new_ret_no_self)]
    fn new(vm: &mut BexVm) -> Value {
        alloc_cancel_token(vm, CancellationToken::new())
    }

    fn any(vm: &mut BexVm, tokens: &[Value]) -> Value {
        // A fresh composite token that fires when any input fires. One watcher
        // task per input links `input.cancelled() -> composite.cancel()`; each
        // watcher self-terminates when the composite is cancelled (directly or
        // by a sibling), so they never outlive the composite. Cancelling the
        // composite does not propagate back to the inputs (one-directional).
        let composite = CancellationToken::new();
        for &token in tokens {
            let Some(input) = as_cancellation_token(vm, token) else {
                continue;
            };
            let out = composite.clone();
            let watcher = async move {
                tokio::select! {
                    biased;
                    () = input.cancelled() => out.cancel(),
                    () = out.cancelled() => {}
                }
            };
            #[cfg(not(target_arch = "wasm32"))]
            tokio::spawn(watcher);
            #[cfg(target_arch = "wasm32")]
            wasm_bindgen_futures::spawn_local(watcher);
        }
        alloc_cancel_token(vm, composite)
    }

    fn cancel(vm: &BexVm, canceltoken: &view::spawn::CancelToken<'_>) -> i64 {
        // Returns 1 if this call performed the Pending -> Cancelled transition,
        // 0 if the token was already cancelled. `CancellationToken::cancel`
        // returns (), so we snapshot `is_cancelled` first (a benign TOCTOU
        // under concurrent cancels — the count is best-effort).
        let token = canceltoken._handle::<CancellationToken>(vm);
        let was_cancelled = token.is_cancelled();
        token.cancel();
        i64::from(!was_cancelled)
    }

    fn is_cancelled(vm: &BexVm, canceltoken: &view::spawn::CancelToken<'_>) -> bool {
        canceltoken._handle::<CancellationToken>(vm).is_cancelled()
    }
}

#[allow(clippy::used_underscore_items)]
impl BamlClassSpawnTaskGroup for PackageBamlImpl {
    // Static constructor: returns the heap `TaskGroup` instance, not `Self`.
    #[allow(clippy::new_ret_no_self)]
    fn new(vm: &mut BexVm, limit: i64, name: Option<&bex_str::BexStr>) -> Value {
        // A negative limit is clamped to 0 (a paused group) rather than
        // throwing — the generated `new` returns `Value`, not a fallible type.
        let limit = usize::try_from(limit).unwrap_or(0);
        let inner = TaskGroupInner::new(limit, name.map(|s| s.as_str().to_owned()));
        alloc_task_group(vm, inner)
    }

    fn cancel(
        vm: &BexVm,
        taskgroup: &view::spawn::TaskGroup<'_>,
        pending: Option<bool>,
        active: Option<bool>,
    ) -> i64 {
        // Both default to `true` (cancel everything) when omitted.
        let group = taskgroup._handle::<TaskGroupInner>(vm);
        clamp_to_i64(group.cancel(pending.unwrap_or(true), active.unwrap_or(true)))
    }

    fn set_limit(vm: &BexVm, taskgroup: &view::spawn::TaskGroup<'_>, limit: i64) {
        taskgroup
            ._handle::<TaskGroupInner>(vm)
            .set_limit(usize::try_from(limit).unwrap_or(0));
    }

    fn limit(vm: &BexVm, taskgroup: &view::spawn::TaskGroup<'_>) -> i64 {
        clamp_to_i64(taskgroup._handle::<TaskGroupInner>(vm).limit())
    }

    fn name(vm: &BexVm, taskgroup: &view::spawn::TaskGroup<'_>) -> Option<bex_str::BexStr> {
        taskgroup
            ._handle::<TaskGroupInner>(vm)
            .name()
            .map(bex_str::BexStr::from)
    }

    fn active_count(vm: &BexVm, taskgroup: &view::spawn::TaskGroup<'_>) -> i64 {
        clamp_to_i64(taskgroup._handle::<TaskGroupInner>(vm).active_count())
    }

    fn queued_count(vm: &BexVm, taskgroup: &view::spawn::TaskGroup<'_>) -> i64 {
        clamp_to_i64(taskgroup._handle::<TaskGroupInner>(vm).queued_count())
    }
}

// The namespace trait is dispatch-only since `options` moved to pure BAML
// (it returns a `SpawnParams` transformer; see ns_spawn/spawn.baml).
impl BamlNamespaceSpawn for PackageBamlImpl {}
