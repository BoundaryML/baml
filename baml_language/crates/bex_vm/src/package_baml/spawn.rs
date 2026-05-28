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

use bex_vm_types::{
    TaskGroupInner,
    types::{CancellationToken, Object, SpawnConfigData, Value},
};

use super::{
    BamlClassSpawnCancelToken, BamlClassSpawnTaskGroup, BamlNamespaceSpawn, PackageBamlImpl,
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
    let handle = vm.alloc_rust_data(Arc::new(token));
    vm.alloc_instance(class, vec![handle])
}

/// Borrow the `TaskGroupInner` behind a `baml.spawn.TaskGroup` receiver, for
/// the read/mutate instance methods (which only need `&TaskGroupInner` —
/// mutation goes through its internal `Mutex`).
fn as_task_group(vm: &BexVm, value: Value) -> Option<&TaskGroupInner> {
    let handle = vm.as_instance(&value).ok()?.load_field(0);
    vm.as_rust_data::<TaskGroupInner>(&handle).ok()
}

/// Clone the `Arc<TaskGroupInner>` behind a `baml.spawn.TaskGroup` value, so it
/// can be shared into a `SpawnConfig` (and onward to the engine's admission).
fn as_task_group_arc(vm: &BexVm, value: Value) -> Option<Arc<TaskGroupInner>> {
    let handle = vm.as_instance(&value).ok()?.load_field(0);
    let ptr = handle.as_object_ptr()?;
    // SAFETY: native functions run inline on the VM thread with the heap
    // permit held, so the pointed-to object is not concurrently moved by GC.
    match unsafe { ptr.get() } {
        Object::RustData(data) => data.clone().downcast::<TaskGroupInner>().ok(),
        _ => None,
    }
}

/// Allocate a fresh `baml.spawn.TaskGroup` instance wrapping `inner`.
fn alloc_task_group(vm: &mut BexVm, inner: Arc<TaskGroupInner>) -> Value {
    let class = vm.resolve_class("baml.spawn.TaskGroup");
    let handle = vm.alloc_rust_data(inner);
    vm.alloc_instance(class, vec![handle])
}

/// Convert a `usize` count/limit to `i64`, saturating at `i64::MAX`. Group
/// counts and limits are always small in practice; this just makes the cast
/// explicitly lossless-or-saturating for clippy.
fn clamp_to_i64(value: usize) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

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

    fn cancel(vm: &BexVm, canceltoken: &Value) -> i64 {
        // Returns 1 if this call performed the Pending -> Cancelled transition,
        // 0 if the token was already cancelled. `CancellationToken::cancel`
        // returns (), so we snapshot `is_cancelled` first (a benign TOCTOU
        // under concurrent cancels — the count is best-effort).
        match as_cancellation_token(vm, *canceltoken) {
            Some(token) => {
                let was_cancelled = token.is_cancelled();
                token.cancel();
                i64::from(!was_cancelled)
            }
            None => 0,
        }
    }

    fn is_cancelled(vm: &BexVm, canceltoken: &Value) -> bool {
        as_cancellation_token(vm, *canceltoken).is_some_and(|t| t.is_cancelled())
    }
}

impl BamlClassSpawnTaskGroup for PackageBamlImpl {
    // Static constructor: returns the heap `TaskGroup` instance, not `Self`.
    #[allow(clippy::new_ret_no_self)]
    fn new(vm: &mut BexVm, limit: i64, name: Option<&str>) -> Value {
        // A negative limit is clamped to 0 (a paused group) rather than
        // throwing — the generated `new` returns `Value`, not a fallible type.
        let limit = usize::try_from(limit).unwrap_or(0);
        let inner = TaskGroupInner::new(limit, name.map(str::to_owned));
        alloc_task_group(vm, inner)
    }

    fn cancel(vm: &BexVm, taskgroup: &Value, pending: Option<bool>, active: Option<bool>) -> i64 {
        // Both default to `true` (cancel everything) when omitted.
        match as_task_group(vm, *taskgroup) {
            Some(group) => {
                clamp_to_i64(group.cancel(pending.unwrap_or(true), active.unwrap_or(true)))
            }
            None => 0,
        }
    }

    fn set_limit(vm: &BexVm, taskgroup: &Value, limit: i64) {
        if let Some(group) = as_task_group(vm, *taskgroup) {
            group.set_limit(usize::try_from(limit).unwrap_or(0));
        }
    }

    fn limit(vm: &BexVm, taskgroup: &Value) -> i64 {
        as_task_group(vm, *taskgroup).map_or(0, |g| clamp_to_i64(g.limit()))
    }

    fn name(vm: &BexVm, taskgroup: &Value) -> Option<String> {
        as_task_group(vm, *taskgroup).and_then(|g| g.name().map(str::to_owned))
    }

    fn active_count(vm: &BexVm, taskgroup: &Value) -> i64 {
        as_task_group(vm, *taskgroup).map_or(0, |g| clamp_to_i64(g.active_count()))
    }

    fn queued_count(vm: &BexVm, taskgroup: &Value) -> i64 {
        as_task_group(vm, *taskgroup).map_or(0, |g| clamp_to_i64(g.queued_count()))
    }
}

impl BamlNamespaceSpawn for PackageBamlImpl {
    fn options(
        vm: &mut BexVm,
        group: Option<&Value>,
        cancel: Option<&Value>,
        detach: Option<bool>,
    ) -> Value {
        // Omitted optional arguments arrive as `None` here.
        let cancel = cancel.and_then(|value| as_cancellation_token(vm, *value));
        let group = group.and_then(|value| as_task_group_arc(vm, *value));
        let detach = detach.unwrap_or(false);
        let class = vm.resolve_class("baml.spawn.SpawnConfig");
        let handle = vm.alloc_rust_data(Arc::new(SpawnConfigData {
            cancel,
            detach,
            group,
        }));
        vm.alloc_instance(class, vec![handle])
    }
}
