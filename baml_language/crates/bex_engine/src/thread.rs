//! `BexThread`: a `BexVm` plus the metadata needed to participate in
//! BEP-034 spawn / await scheduling.
//!
//! Phase A only introduces the wrapper. The engine still runs a single
//! root thread per `call_function` and there is no behavior change. Phase
//! B adds child threads and routes child completions through
//! `settles_future`.

use std::{collections::HashMap, sync::Arc};

use ::bex_heap::{Tlab, TlabHolder};
use ::bex_vm_types::{HeapPtr, RootHaver, types::FutureId};
use bex_vm::BexVm;
use tokio_util::sync::CancellationToken;

/// What cancels a task: its own token, and every token linked into it.
///
/// `own` is what the task's future cancels, what its parent's cancellation
/// reaches (it is a child token of the parent's), and what the task fires when
/// it fails or is cancelled, so the cancellation cascades to its children.
/// `linked` holds the tokens its plan links in (`spawn with token`) and those
/// it inherits from its parent, so a linked token reaches every descendant for
/// as long as that descendant runs, including after the task that linked it
/// has finished. The task is cancelled once any of them has fired.
#[derive(Clone)]
pub struct TaskCancel {
    own: CancellationToken,
    linked: Arc<[CancellationToken]>,
    deadline: Option<crate::invocation::InvocationDeadline>,
}

impl TaskCancel {
    /// A root VM's private cancellation authority, before inputs are attached.
    pub fn root(own: CancellationToken) -> Self {
        Self {
            own,
            linked: Arc::from([]),
            deadline: None,
        }
    }

    /// A task with no cancellation parent (a rooted spawn, or one started by
    /// cleanup code): a fresh token, and `linked`.
    pub fn detached(linked: impl IntoIterator<Item = CancellationToken>) -> Self {
        Self {
            own: CancellationToken::new(),
            linked: linked.into_iter().collect(),
            deadline: None,
        }
    }

    /// A child task's: cancelled with this task, and by `linked` too.
    #[must_use]
    pub fn child(&self, linked: impl IntoIterator<Item = CancellationToken>) -> Self {
        Self {
            own: self.own.child_token(),
            linked: self.linked.iter().cloned().chain(linked).collect(),
            deadline: self.deadline,
        }
    }

    /// The task's own token: what its future's `cancel()` fires.
    pub fn own(&self) -> &CancellationToken {
        &self.own
    }

    /// Every token whose firing cancels the task.
    pub fn tokens(&self) -> impl Iterator<Item = &CancellationToken> {
        std::iter::once(&self.own).chain(self.linked.iter())
    }

    pub fn is_cancelled(&self) -> bool {
        self.tokens().any(CancellationToken::is_cancelled)
            || self
                .deadline
                .is_some_and(crate::invocation::InvocationDeadline::is_expired)
    }

    pub(crate) fn with_deadline(
        mut self,
        deadline: Option<crate::invocation::InvocationDeadline>,
    ) -> Self {
        self.deadline = deadline;
        self
    }

    pub(crate) fn deadline(&self) -> Option<crate::invocation::InvocationDeadline> {
        self.deadline
    }

    /// Completes once any token that cancels the task has fired.
    pub async fn cancelled(&self) {
        let sources = async {
            if self.linked.is_empty() {
                self.own.cancelled().await;
            } else {
                futures::future::select_all(self.tokens().map(|token| Box::pin(token.cancelled())))
                    .await;
            }
        };
        match self.deadline {
            Some(deadline) => {
                tokio::select! { () = sources => {}, () = deadline.elapsed() => {} }
            }
            None => sources.await,
        }
    }

    /// Cancel the task and, through its own token, its descendants.
    pub fn cancel(&self) {
        self.own.cancel();
    }
}

/// A BEX virtual-machine instance plus the scheduling metadata that
/// distinguishes a root call from a spawned child.
pub struct BexThread {
    pub vm: BexVm,
    pub name: Option<String>,
    pub cancel: TaskCancel,
    /// The token the thread's in-flight sys-op observes — never `cancel`
    /// itself: whether the thread is shielded is read only once `cancel`
    /// fires, and a shielded thread's cleanup runs its sys-ops to
    /// completion. Fired, and replaced, when cancellation is delivered to
    /// an op in flight, so the abandoned op stops and later ops start clean.
    pub sysop_cancel: CancellationToken,
    pub settles_future: Option<FutureId>,
    /// How the error that escaped this root thread ends it, for telemetry.
    pub escaped_outcome: Option<bex_vm::telemetry::InvocationOutcome>,
    /// The network span the failing sys-op ends: closed with the thrown
    /// value once the engine has built it.
    pub(crate) network_close: Option<crate::telemetry_network::NetworkClose>,
}

impl BexThread {
    /// Build a root thread with no future to settle.
    pub fn new_root(vm: BexVm, cancel: TaskCancel) -> Self {
        Self {
            vm,
            name: None,
            cancel,
            sysop_cancel: CancellationToken::new(),
            settles_future: None,
            escaped_outcome: None,
            network_close: None,
        }
    }

    /// Build a child thread that will settle `future_id` when its body terminates.
    pub fn new_child(
        vm: BexVm,
        cancel: TaskCancel,
        name: Option<String>,
        settles_future: FutureId,
    ) -> Self {
        Self {
            vm,
            name,
            cancel,
            sysop_cancel: CancellationToken::new(),
            settles_future: Some(settles_future),
            escaped_outcome: None,
            network_close: None,
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

    /// What cancels this thread.
    pub fn vm_thread_cancel(&self) -> &TaskCancel {
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
