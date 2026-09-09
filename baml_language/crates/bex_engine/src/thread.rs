//! `BexThread`: a `BexVm` plus the metadata needed to participate in
//! BEP-034 spawn / await scheduling.
//!
//! Root threads retain the host-facing output contract; child threads route
//! their completion through `settles_future`. Both participate in heap tracing.

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
    /// Exact host-facing contract. Declaration pointers stay rooted across
    /// suspension and moving GC; wire names are presentation, not identity.
    pub(crate) outbound: Option<OutboundContract>,
    /// The currently suspended host callback's exact result/error contracts.
    /// Both root and child threads must trace these across moving GC.
    pub(crate) host_call: Option<OutboundContract>,
}

pub(crate) struct OutboundContract {
    pub returns: bex_vm_types::RuntimeTy,
    pub throws: Option<bex_vm_types::RuntimeTy>,
}

impl BexThread {
    /// Build a root thread with no future to settle.
    pub fn new_root(vm: BexVm, cancel: CancellationToken) -> Self {
        Self {
            vm,
            name: None,
            cancel,
            settles_future: None,
            outbound: None,
            host_call: None,
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
            outbound: None,
            host_call: None,
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
}

impl RootHaver for BexThread {
    fn collect_roots(&self, roots: &mut Vec<HeapPtr>) {
        self.vm.collect_roots(roots);
        for contract in self.outbound.iter().chain(self.host_call.iter()) {
            for ty in std::iter::once(&contract.returns).chain(contract.throws.iter()) {
                ty.visit_heads(&mut |head| {
                    if head.is_resolved() {
                        roots.push(head.ptr());
                    }
                });
            }
        }
    }

    fn forward_roots(&mut self, roots: &HashMap<HeapPtr, HeapPtr>) {
        self.vm.forward_roots(roots);
        for contract in self.outbound.iter_mut().chain(self.host_call.iter_mut()) {
            for ty in std::iter::once(&mut contract.returns).chain(contract.throws.iter_mut()) {
                ty.visit_heads_mut(&mut |head| {
                    if head.is_resolved()
                        && let Some(&moved) = roots.get(&head.ptr())
                    {
                        head.forward_to(moved);
                    }
                });
            }
        }
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
