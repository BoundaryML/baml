use std::collections::HashMap;

use bex_vm_types::{
    Future, HeapPtr,
    types::{FutureId, Object, Value},
};

use super::{BamlNamespaceMock, Continuation, NativeCallResult, PackageBamlImpl};
use crate::BexVm;

impl BamlNamespaceMock for PackageBamlImpl {
    /// Push a `target → replacement` override onto the VM's mock stack.
    ///
    /// Both `target` and `replacement` must be function values (typically
    /// `Object::Function`, `Object::Closure`, or `Object::BoundMethod`). The
    /// `HeapPtr` identity of the target is what the interception path
    /// matches against in `Instruction::Call`, `Instruction::CallIndirect`,
    /// and `Instruction::DispatchFuture`.
    fn push_override(vm: &mut BexVm, target: &Value, replacement: &Value) {
        if let (Value::Object(t), Value::Object(r)) = (target, replacement) {
            // Normalize the target to its underlying function identity so a
            // Closure-wrapped value pushed here matches a raw Function loaded
            // at the call site (and vice-versa). The replacement is left
            // un-normalized — Closures with captures are valid replacements
            // and must be invoked with their captures intact.
            let target_id = vm.callable_identity(*t);
            vm.mock_stack.push((target_id, *r));
        }
    }

    fn pop_override(vm: &mut BexVm) {
        vm.mock_stack.pop();
    }
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
