//! `BexThread`: a `BexVm` plus the metadata needed to participate in
//! BEP-034 spawn / await scheduling.
//!
//! Phase A only introduces the wrapper. The engine still runs a single
//! root thread per `call_function` and there is no behavior change. Phase
//! B adds child threads and routes child completions through
//! `settles_future`.

use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
};

use ::bex_heap::{Tlab, TlabHolder};
use ::bex_vm_types::{HeapPtr, RootHaver, types::FutureId};
use bex_vm::BexVm;
use tokio_util::sync::CancellationToken;

/// Cross-thread FIFO of children futures that errored fire-and-forget —
/// i.e. they reached a terminal `Error` or `Cancelled` state without an
/// explicit `await` consuming the error.
///
/// Per BEP-034 ("If a fire-and-forget task throws an unhandled error,
/// the error propagates to the parent task at its next `await` point"),
/// every `BexThread` owns one of these as `pending_child_errors`, and
/// each child thread holds a clone of its parent's queue in
/// `parent_pending_errors`. When the child terminates with an error
/// it pushes its `(FutureId, HeapPtr)` onto the parent's queue. The
/// parent's next engine yield (await / sys-op) drains one entry and
/// raises the underlying value at that yield point.
///
/// Entries are `(FutureId, HeapPtr)` pairs. The `FutureId` is stable
/// for the lifetime of the future and is the right key for the
/// "explicitly awaited future" carve-out (it doesn't get rewritten by
/// GC and doesn't change when the producer settles + removes the
/// `active_futures` bookkeeping entry). The `HeapPtr` is needed to
/// read the future's terminal `Value` when surfacing an unhandled
/// error to the host.
///
/// GC: while a future ptr sits in the queue it is rooted via the
/// owning `BexThread::collect_roots`, which locks the queue and
/// pushes the `HeapPtr`s. `forward_roots` mirrors the update path. The
/// `std::sync::Mutex` is held only across brief push/pop/iterate
/// critical sections that never `.await`, so it cannot deadlock with
/// the tokio runtime.
pub struct ChildErrorQueue {
    inner: Mutex<VecDeque<(FutureId, HeapPtr)>>,
}

impl ChildErrorQueue {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(VecDeque::new()),
        })
    }

    /// Append a `(future_id, heap_ptr)` entry. The future must be in
    /// `Error` or `Cancelled` state. Called by a child thread's
    /// termination path on its parent's queue.
    pub fn push(&self, future_id: FutureId, future_ptr: HeapPtr) {
        self.inner
            .lock()
            .expect("ChildErrorQueue poisoned")
            .push_back((future_id, future_ptr));
    }

    /// Pop the oldest queued entry, or `None` if empty.
    pub fn pop(&self) -> Option<(FutureId, HeapPtr)> {
        self.inner
            .lock()
            .expect("ChildErrorQueue poisoned")
            .pop_front()
    }

    /// Remove every queue entry whose future id equals `id`. Returns
    /// the number removed. Used by the parent's await handler to
    /// "consume" an explicitly-awaited errored future from the queue —
    /// the VM will throw via the heap state on its own, letting user
    /// `catch` clauses fire naturally rather than being short-circuited
    /// by the engine-level fire-and-forget surface path.
    ///
    /// Keying on `FutureId` (not `HeapPtr`) is what makes the carve-out
    /// robust: a producer settle that removes the `active_futures`
    /// entry, or a GC that relocates the heap object during the await
    /// wait, both leave the `future_id` unchanged.
    pub fn remove_matching(&self, id: FutureId) -> usize {
        let mut queue = self.inner.lock().expect("ChildErrorQueue poisoned");
        let before = queue.len();
        queue.retain(|(qid, _)| *qid != id);
        before - queue.len()
    }

    /// GC root collection: copy out every queued ptr.
    pub fn collect_roots(&self, roots: &mut Vec<HeapPtr>) {
        let queue = self.inner.lock().expect("ChildErrorQueue poisoned");
        roots.extend(queue.iter().map(|(_, ptr)| *ptr));
    }

    /// GC fixup: rewrite queued ptrs through the relocation mapping.
    pub fn forward_roots(&self, mapping: &HashMap<HeapPtr, HeapPtr>) {
        let mut queue = self.inner.lock().expect("ChildErrorQueue poisoned");
        for (_, ptr) in queue.iter_mut() {
            if let Some(new_ptr) = mapping.get(ptr) {
                *ptr = *new_ptr;
            }
        }
    }
}

/// A BEX virtual-machine instance plus the scheduling metadata that
/// distinguishes a root call from a spawned child.
pub struct BexThread {
    pub vm: BexVm,
    pub name: Option<String>,
    pub cancel: CancellationToken,
    pub settles_future: Option<FutureId>,
    /// Queue of errored child futures (heap ptrs) that this thread's
    /// children push onto when they terminate fire-and-forget with an
    /// unhandled throw. Drained at this thread's next engine yield to
    /// propagate the error per BEP-034. Always non-None so children
    /// always have a valid push target.
    pub pending_child_errors: Arc<ChildErrorQueue>,
    /// Clone of the parent's `pending_child_errors`, or `None` for the
    /// root thread. When this thread terminates with an unhandled throw,
    /// the engine pushes our settled future ptr here so the parent
    /// observes the error at its next await.
    pub parent_pending_errors: Option<Arc<ChildErrorQueue>>,
}

impl BexThread {
    /// Build a root thread — no parent, no future to settle.
    pub fn new_root(vm: BexVm, cancel: CancellationToken) -> Self {
        Self {
            vm,
            name: None,
            cancel,
            settles_future: None,
            pending_child_errors: ChildErrorQueue::new(),
            parent_pending_errors: None,
        }
    }

    /// Build a child thread that will settle `future_id` when its body
    /// terminates. `parent_pending_errors` is a clone of the spawning
    /// thread's `pending_child_errors` queue Arc.
    pub fn new_child(
        vm: BexVm,
        cancel: CancellationToken,
        name: Option<String>,
        settles_future: FutureId,
        parent_pending_errors: Arc<ChildErrorQueue>,
    ) -> Self {
        Self {
            vm,
            name,
            cancel,
            settles_future: Some(settles_future),
            pending_child_errors: ChildErrorQueue::new(),
            parent_pending_errors: Some(parent_pending_errors),
        }
    }

    /// The future this thread settles on termination, if it is a spawned
    /// child. Named with the `vm_thread_` prefix so the call site reads
    /// clearly through an `ActiveHeapPermit<BexThread>` deref.
    pub fn vm_thread_settles_future(&self) -> Option<FutureId> {
        self.settles_future
    }

    /// This thread's own cancellation token.
    pub fn vm_thread_cancel(&self) -> &CancellationToken {
        &self.cancel
    }

    /// Pop one queued child-error entry, if any. Called by the engine
    /// at this thread's await/sys-op checkpoints to deliver
    /// fire-and-forget errors per BEP-034. Returns the future's id and
    /// its heap ptr; the engine reads the terminal `Value` from the
    /// heap object.
    pub fn vm_thread_pop_pending_child_error(&self) -> Option<(FutureId, HeapPtr)> {
        self.pending_child_errors.pop()
    }

    /// Remove any queue entries for `future_id` (the explicitly-awaited
    /// future). Returns true if at least one entry was removed.
    /// See [`ChildErrorQueue::remove_matching`] for why we key on the
    /// future id rather than the heap ptr.
    pub fn vm_thread_consume_pending_child_error_for(&self, future_id: FutureId) -> bool {
        self.pending_child_errors.remove_matching(future_id) > 0
    }

    /// Push our settled `(future_id, future_ptr)` onto the parent's
    /// pending-child-errors queue, if we have a parent. No-op for root
    /// threads.
    pub fn vm_thread_notify_parent_of_error(
        &self,
        settled_future_id: FutureId,
        settled_future_ptr: HeapPtr,
    ) {
        if let Some(parent_q) = &self.parent_pending_errors {
            parent_q.push(settled_future_id, settled_future_ptr);
        }
    }

    /// Clone of this thread's `pending_child_errors` Arc. Passed to
    /// freshly spawned children as their `parent_pending_errors`.
    pub fn vm_thread_pending_errors_arc(&self) -> Arc<ChildErrorQueue> {
        Arc::clone(&self.pending_child_errors)
    }
}

impl RootHaver for BexThread {
    fn collect_roots(&self, roots: &mut Vec<HeapPtr>) {
        self.vm.collect_roots(roots);
        // The pending-child-errors queue holds settled future ptrs
        // until the parent's next yield drains them; keep those heap
        // objects alive across any concurrent GC.
        self.pending_child_errors.collect_roots(roots);
    }

    fn forward_roots(&mut self, roots: &HashMap<HeapPtr, HeapPtr>) {
        self.vm.forward_roots(roots);
        self.pending_child_errors.forward_roots(roots);
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
