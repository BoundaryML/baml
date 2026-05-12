//! `BexThread`: a `BexVm` plus the metadata needed to participate in
//! BEP-034 spawn / await scheduling.
//!
//! Phase A only introduces the wrapper. The engine still runs a single
//! root thread per `call_function` and there is no behavior change. Phase
//! B adds child threads and routes child completions through
//! `settles_future`.

#![allow(dead_code, unreachable_pub)]

use std::{
    collections::HashMap,
    sync::atomic::{AtomicU64, Ordering},
};

use ::bex_heap::{Tlab, TlabHolder};
use ::bex_vm_types::{HeapPtr, RootHaver, types::FutureId};
use bex_vm::BexVm;
use tokio_util::sync::CancellationToken;

/// Identifier for a BEX-level green thread.
///
/// Distinct from a tokio task id: stable across the lifetime of a single
/// `BexThread` and used to track parent/child relationships.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct ThreadId(u64);

impl ThreadId {
    pub fn next() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        ThreadId(NEXT.fetch_add(1, Ordering::Relaxed))
    }

    pub fn raw(self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for ThreadId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "thread#{}", self.0)
    }
}

/// A BEX virtual-machine instance plus the scheduling metadata that
/// distinguishes a root call from a spawned child.
pub struct BexThread {
    pub vm: BexVm,
    pub id: ThreadId,
    pub name: Option<String>,
    pub cancel: CancellationToken,
    pub settles_future: Option<FutureId>,
    pub parent: Option<ThreadId>,
    // v2 expansion points (intentionally left commented):
    // pub group: Option<TaskGroupHandle>,
    // pub detach: bool,
}

impl BexThread {
    /// Build a root thread — no parent, no future to settle.
    pub fn new_root(vm: BexVm, cancel: CancellationToken) -> Self {
        Self {
            vm,
            id: ThreadId::next(),
            name: None,
            cancel,
            settles_future: None,
            parent: None,
        }
    }

    /// Build a child thread that will settle `future_id` when its body
    /// terminates.
    pub fn new_child(
        vm: BexVm,
        cancel: CancellationToken,
        name: Option<String>,
        settles_future: FutureId,
        parent: ThreadId,
    ) -> Self {
        Self {
            vm,
            id: ThreadId::next(),
            name,
            cancel,
            settles_future: Some(settles_future),
            parent: Some(parent),
        }
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
