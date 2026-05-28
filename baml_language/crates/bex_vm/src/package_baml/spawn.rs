//! Native implementations for `namespace baml.spawn` (BEP-034 spawn options):
//! the `CancelToken` class and the `options(...)` factory.
//!
//! `CancelToken` is an opaque class whose `_handle` field holds an
//! `Object::RustData(Arc<tokio_util::sync::CancellationToken>)`. `SpawnConfig`
//! is likewise opaque, holding an `Arc<SpawnConfigData>`. Both are built with
//! the standard `resolve_class` + `alloc_rust_data` + `alloc_instance` pattern
//! and read back via `as_instance` + `load_field(0)` + `as_rust_data`.

use std::sync::Arc;

use bex_vm_types::types::{CancellationToken, SpawnConfigData, Value};

use super::{BamlClassSpawnCancelToken, BamlNamespaceSpawn, PackageBamlImpl};
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

impl BamlNamespaceSpawn for PackageBamlImpl {
    fn options(vm: &mut BexVm, _group: Option<&Value>, cancel: Option<&Value>) -> Value {
        // PR1: only `cancel` is honored at runtime; `group` is accepted for
        // forward compatibility and wired in a follow-up. An omitted optional
        // argument arrives as the `OmittedArg` sentinel, which is neither null
        // nor a `CancelToken` instance, so `as_cancellation_token` resolves it
        // to `None` — exactly the "no cancel token" case.
        let cancel = cancel.and_then(|value| as_cancellation_token(vm, *value));
        let class = vm.resolve_class("baml.spawn.SpawnConfig");
        let handle = vm.alloc_rust_data(Arc::new(SpawnConfigData { cancel }));
        vm.alloc_instance(class, vec![handle])
    }
}
