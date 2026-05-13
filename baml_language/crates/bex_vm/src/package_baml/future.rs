#![allow(unsafe_code)]

//! Native implementations of `class baml.future.Future` methods.
//!
//! Every method receives the future as `&Value` (the receiver) and operates
//! on the heap `Object::Future` directly. The atomic state machine + wake
//! `SetOnce` + cancel `CancellationToken` all live on the heap object itself
//! (see `bex_vm_types::Future`), so these methods are purely synchronous —
//! no engine round-trip, no `FutureManager` lookup.
//!
//! All transitions (`cancel`) use the CAS-based `settle_*` methods on
//! `Future`, which race correctly with the producer's settle from another
//! thread: the first writer to CAS `state` wins, others observe the new
//! state and become no-ops.

use bex_vm_types::types::{FutureRead, Object, Value};

use super::{BamlClassFutureFuture, BamlNamespaceFuture, PackageBamlImpl};
use crate::BexVm;

/// Reach into the receiver to get `&Future`. Returns `None` if the value
/// isn't actually an `Object::Future` (defensive — the compiler enforces
/// the receiver type, so a non-Future would be a VM bug).
fn as_future(value: &Value) -> Option<&bex_vm_types::Future> {
    let Value::Object(ptr) = *value else {
        return None;
    };
    // SAFETY: native functions run inline on the VM thread while the heap
    // permit is held; the heap object pointed to is not concurrently moved
    // by GC. The compiler guarantees the receiver has type `Future<T,E>`,
    // so `Object::Future` is the only legal payload.
    match unsafe { ptr.get() } {
        Object::Future(f) => Some(f),
        _ => None,
    }
}

impl BamlClassFutureFuture for PackageBamlImpl {
    fn cancel(future: &Value) -> bool {
        let Some(fut) = as_future(future) else {
            return false;
        };
        // CAS Pending → Cancelled inside settle_cancelled; also fires the
        // cancel token (so the producer's next await checkpoint observes
        // it) and the SetOnce (so any current awaiter resumes).
        fut.settle_cancelled()
    }

    fn is_settled(future: &Value) -> bool {
        let Some(fut) = as_future(future) else {
            return false;
        };
        !matches!(fut.read(), FutureRead::Pending(_))
    }

    fn is_cancelled(future: &Value) -> bool {
        let Some(fut) = as_future(future) else {
            return false;
        };
        matches!(fut.read(), FutureRead::Cancelled)
    }

    fn is_result(future: &Value) -> bool {
        let Some(fut) = as_future(future) else {
            return false;
        };
        matches!(fut.read(), FutureRead::Ready(_))
    }

    fn is_error(future: &Value) -> bool {
        let Some(fut) = as_future(future) else {
            return false;
        };
        // Surface `InternalError` as `is_error()` too — both are "future
        // failed" from the user's point of view; the user-facing enum
        // `FutureState` has no separate `InternalError` variant.
        matches!(
            fut.read(),
            FutureRead::Error(_) | FutureRead::InternalError(_)
        )
    }

    fn state(vm: &mut BexVm, future: &Value) -> Value {
        let Some(fut) = as_future(future) else {
            return Value::Null;
        };
        let variant_name = match fut.read() {
            FutureRead::Pending(_) => "Pending",
            FutureRead::Ready(_) => "Ready",
            // Surface InternalError as Error — see is_error.
            FutureRead::Error(_) | FutureRead::InternalError(_) => "Error",
            FutureRead::Cancelled => "Cancelled",
        };
        let Some(enm_ptr) = vm
            .resolved_class_names
            .get("baml.future.FutureState")
            .copied()
        else {
            return Value::Null;
        };
        // SAFETY: enm_ptr came from resolved_class_names which holds
        // compile-time-registered HeapPtrs valid for the program's lifetime.
        let idx = match unsafe { enm_ptr.get() } {
            Object::Enum(e) => e.variants.iter().position(|v| v.name == variant_name),
            _ => None,
        };
        match idx {
            Some(i) => vm.alloc_variant(enm_ptr, i),
            None => Value::Null,
        }
    }
}

impl BamlNamespaceFuture for PackageBamlImpl {}
