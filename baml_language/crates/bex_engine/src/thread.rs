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
    pub settles_future: Option<FutureId>,
    /// Monotonic sequence for this logical thread's profiling park records.
    ///
    /// This lives on the logical thread rather than in task-local/TLS state:
    /// Tokio may migrate the task at any `.await`, while a suspend/resume pair
    /// must keep one identity across that migration. Zero is reserved as the
    /// "not emitted" sentinel; the first emitted suspension is sequence 1.
    prof_suspend_seq: u32,
}

impl BexThread {
    /// Build a root thread with no future to settle.
    pub fn new_root(vm: BexVm, cancel: CancellationToken) -> Self {
        Self {
            vm,
            name: None,
            cancel,
            settles_future: None,
            prof_suspend_seq: 0,
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
            settles_future: Some(settles_future),
            prof_suspend_seq: 0,
        }
    }

    /// Allocate the next profiler suspension sequence.
    ///
    /// A `u32` sequence is part of the frozen raw format. In the practically
    /// unreachable event that one logical thread parks `u32::MAX` times, stop
    /// emitting suspend records instead of wrapping and violating monotonicity.
    pub(crate) fn next_prof_suspend_seq(&mut self) -> Option<u32> {
        let next = self.prof_suspend_seq.checked_add(1)?;
        self.prof_suspend_seq = next;
        Some(next)
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
