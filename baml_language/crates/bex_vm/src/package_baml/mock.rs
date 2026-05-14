use std::collections::HashMap;

use bex_vm_types::{
    Future, HeapPtr,
    types::{FutureId, Object, UnmockedRef, Value},
};

use super::{BamlNamespaceMock, Continuation, NativeCallResult, PackageBamlImpl};
use crate::{
    BexVm,
    errors::{VmBamlError, VmRustFnError},
};

impl BamlNamespaceMock for PackageBamlImpl {
    fn push_override(
        vm: &mut BexVm,
        target: &Value,
        replacement: &Value,
        counter_owner: Option<&Value>,
    ) {
        if let (Value::Object(t), Value::Object(r)) = (target, replacement) {
            // Decompose the target into (function ptr, optional receiver):
            // a `BoundMethod` carries an instance and produces a
            // per-instance override; everything else is a wildcard match
            // against the function identity (covers Function, Closure, and
            // class-method-as-value).
            let (target_fn, receiver) = vm.callable_identity_with_receiver(*t);
            // `counter_owner` is the `Mock<T>` instance whose `call_count`
            // field gets bumped on each match. The BAML signature types it
            // as `Mock<T>?` so callers can opt out by passing `null`.
            let counter_instance = match counter_owner {
                Some(Value::Object(o)) => Some(*o),
                _ => None,
            };
            vm.mock_stack.push(crate::vm::MockOverride {
                target_fn,
                receiver,
                replacement: *r,
                counter_instance,
                suppressed: false,
            });
        }
    }

    fn pop_override(vm: &mut BexVm) {
        vm.mock_stack.pop();
    }

    /// Allocate an `UnmockedRef` wrapping the given callable. Returned to
    /// BAML as type `T` (the wrapped callable's type) — the bypass
    /// behaviour lives in `CallIndirect`/`YieldToCall`'s recognition of
    /// `Object::UnmockedRef`, not in the type system.
    ///
    /// Runtime guard: BAML doesn't (yet) support generic bounds — there's
    /// no way to say `Mock<T> where T: callable` à la Rust's `T: Fn(...)`
    /// or C++'s `requires invocable<T>`. So nothing at the type level
    /// rejects `Mock.new(42)`; we catch that misuse here at construction
    /// time with a panic. When BAML gains generic constraints, this
    /// check (and the `throws baml.panics.UserPanic` on the BAML
    /// declaration) can be deleted in favour of a compile-time
    /// `T: callable` bound.
    fn wrap_unmocked(vm: &mut BexVm, target: &Value) -> Result<Value, VmRustFnError> {
        let inner = match target {
            Value::Object(p) => match vm.get_object(*p) {
                Object::Function(_) | Object::Closure(_) | Object::BoundMethod(_) => *p,
                other => {
                    return Err(VmBamlError::InvalidArgument {
                        message: format!(
                            "baml.mock.Mock.new: target is not callable (got {})",
                            ::bex_vm_types::types::ObjectType::of(other)
                        ),
                    }
                    .into());
                }
            },
            _ => {
                return Err(VmBamlError::InvalidArgument {
                    message: "baml.mock.Mock.new: target is not callable (got a non-object value)"
                        .to_string(),
                }
                .into());
            }
        };
        let ptr = vm.tlab.alloc(Object::UnmockedRef(UnmockedRef { inner }));
        Ok(Value::Object(ptr))
    }
}

/// Continuation used after every mock-intercepted call returns: flips the
/// matched entry's `suppressed` flag back to `false` so subsequent
/// (outside-the-replacement) calls to the target match the override
/// again. The entry index is captured at intercept time and is stable
/// for the duration of the active scope (`mock_stack` is LIFO push/pop and
/// the entry isn't removed until its scope ends).
///
/// If the matched entry has been popped by the time the continuation
/// runs (e.g. an inner scope is unwinding and `pop_override` was called
/// before this continuation), the unsuppress is a no-op.
pub(crate) struct UnsuppressMockContinuation {
    pub entry_idx: usize,
}

impl Continuation for UnsuppressMockContinuation {
    fn call(self: Box<Self>, vm: &mut BexVm, value: Value) -> NativeCallResult {
        if let Some(entry) = vm.mock_stack.get_mut(self.entry_idx) {
            entry.suppressed = false;
        }
        NativeCallResult::Done(value)
    }

    fn gc_roots(&self) -> Vec<HeapPtr> {
        vec![]
    }

    fn apply_forwarding(&mut self, _: &HashMap<HeapPtr, HeapPtr>) {}
}

/// Continuation used when a `DispatchFuture` is intercepted by a mock: after
/// the replacement closure returns its value, this wraps it in a freshly
/// allocated ready `Future` so the caller's subsequent `Await` instruction
/// sees the expected type and unwraps it synchronously.
pub(crate) struct WrapReadyFutureContinuation;

impl Continuation for WrapReadyFutureContinuation {
    #[allow(unsafe_code)]
    fn call(self: Box<Self>, vm: &mut BexVm, value: Value) -> NativeCallResult {
        // Allocate a Pending future first so we have a stable HeapPtr to
        // pass to `set_ready`'s write barrier.
        let future_ptr = vm
            .tlab
            .alloc(Object::Future(Future::pending(FutureId::from_usize(0))));
        let Object::Future(fut) = (unsafe { future_ptr.get() }) else {
            unreachable!("just allocated as Future");
        };
        // SAFETY: single-writer — we just allocated `future_ptr`, no other
        // reader/writer can see this Future yet.
        unsafe { fut.set_ready(vm.heap.as_ref(), future_ptr, value) };
        NativeCallResult::Done(Value::Object(future_ptr))
    }

    fn gc_roots(&self) -> Vec<HeapPtr> {
        vec![]
    }

    fn apply_forwarding(&mut self, _: &HashMap<HeapPtr, HeapPtr>) {}
}
