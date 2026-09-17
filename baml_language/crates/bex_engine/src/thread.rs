//! `BexThread`: a `BexVm` plus the metadata needed to participate in
//! BEP-034 spawn / await scheduling.
//!
//! Phase A only introduces the wrapper. The engine still runs a single
//! root thread per `call_function` and there is no behavior change. Phase
//! B adds child threads and routes child completions through
//! `settles_future`.

use std::collections::HashMap;

use ::bex_heap::{Tlab, TlabHolder};
use ::bex_vm_types::{HeapPtr, RootHaver, types::FutureId};
use bex_vm::BexVm;
use tokio_util::sync::CancellationToken;

/// A BEX virtual-machine instance plus the scheduling metadata that
/// distinguishes a root call from a spawned child.
pub struct BexThread {
    pub vm: BexVm,
    pub name: Option<String>,
    pub cancel: CancellationToken,
    /// The token the thread's in-flight sys-op observes — never `cancel`
    /// itself: whether the thread is shielded is read only once `cancel`
    /// fires, and a shielded thread's cleanup runs its sys-ops to
    /// completion. Fired, and replaced, when cancellation is delivered to
    /// an op in flight, so the abandoned op stops and later ops start clean.
    pub sysop_cancel: CancellationToken,
    pub settles_future: Option<FutureId>,
}

impl BexThread {
    /// Build a root thread with no future to settle.
    pub fn new_root(vm: BexVm, cancel: CancellationToken) -> Self {
        Self {
            vm,
            name: None,
            cancel,
            sysop_cancel: CancellationToken::new(),
            settles_future: None,
        }
    }

    /// Build a child thread that will settle `future_id` when its body terminates.
    pub fn new_child(
        vm: BexVm,
        cancel: CancellationToken,
        name: Option<String>,
        settles_future: FutureId,
    ) -> Self {
        Self {
            vm,
            name,
            cancel,
            sysop_cancel: CancellationToken::new(),
            settles_future: Some(settles_future),
        }
    }

    /// The future this thread settles on termination, if it is a spawned
    /// child. Named with the `vm_thread_` prefix so the call site reads
    /// clearly through an `ActiveHeapPermit<BexThread>` deref.
    pub fn vm_thread_settles_future(&self) -> Option<FutureId> {
        self.settles_future
    }

    /// Deliver cancellation to the sys-op in flight: fire its token so the
    /// abandoned op stops, and give the ops that follow a fresh one.
    pub fn vm_thread_abort_sysop(&mut self) {
        std::mem::replace(&mut self.sysop_cancel, CancellationToken::new()).cancel();
    }

    /// This thread's own cancellation token.
    pub fn vm_thread_cancel(&self) -> &CancellationToken {
        &self.cancel
    }

    /// Whether the VM is inside a `defer` body, at any depth of the call
    /// stack. Each bytecode frame's function carries the PC ranges of its
    /// `defer` bodies (`Bytecode::shield_table`); the innermost frame is
    /// at `cur_pc`, and an outer frame is at its call instruction: the
    /// saved instruction pointer is the end of that instruction, and the
    /// block the call continues to may already lie outside the body
    /// (`defer { f() }` continues after it), so the call itself decides.
    /// A callee is shielded through its caller, so unwinding needs no
    /// bookkeeping.
    pub fn vm_thread_is_shielded(&self) -> bool {
        let innermost = self
            .vm
            .frames
            .iter()
            .rposition(|frame| matches!(frame, bex_vm::Frame::Bytecode(_)));
        self.vm
            .frames
            .iter()
            .enumerate()
            .rev()
            .any(|(index, frame)| {
                let bex_vm::Frame::Bytecode(frame) = frame else {
                    return false;
                };
                let pc = if Some(index) == innermost {
                    self.vm.cur_pc
                } else {
                    frame.instruction_ptr.checked_sub(1).unwrap_or_else(|| {
                        unreachable!("an outer bytecode frame has executed its call instruction")
                    })
                };
                let function = self.frame_function(frame.function);
                match &function.bytecode.compact {
                    Some(compact) => compact.pc_is_shielded(pc),
                    None => function.bytecode.pc_is_shielded(pc),
                }
            })
    }

    /// The function a bytecode frame runs, resolved as the VM resolves it: a
    /// function, a closure's or bound method's function, or the authored
    /// function behind a generic-function value.
    fn frame_function(&self, function: HeapPtr) -> &::bex_vm_types::types::Function {
        use bex_vm::types::ObjectTrait as _;

        let resolved = match self.vm.get_object(function) {
            ::bex_vm_types::Object::GenericFunction(generic) => self
                .vm
                .generic_function_authored_ptr(generic)
                .and_then(|authored| self.vm.get_object(authored).as_function()),
            other => other.as_callable(),
        };
        resolved.unwrap_or_else(|err| unreachable!("a bytecode frame runs a callable: {err}"))
    }
}

impl RootHaver for BexThread {
    fn collect_roots(&self, roots: &mut Vec<HeapPtr>) {
        self.vm.collect_roots(roots);
    }

    fn forward_roots(&mut self, roots: &HashMap<HeapPtr, HeapPtr>) {
        self.vm.forward_roots(roots);
    }
}

impl TlabHolder for BexThread {
    fn tlab(&self) -> &Tlab {
        self.vm.tlab()
    }

    fn tlab_mut(&mut self) -> &mut Tlab {
        self.vm.tlab_mut()
    }
}
