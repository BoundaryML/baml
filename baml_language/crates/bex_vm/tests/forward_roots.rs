//! Tests for `<BexVm as RootHaver>::forward_roots`.
//!
//! The engine's GC coordinator calls `forward_roots` on every parked VM
//! after a generational collection. Beyond rewriting heap pointers, this
//! hook is also responsible for invalidating the VM's TLAB — after GC, the
//! heap-wide `gen0_next_chunk` cursor is reset, so the VM's cached
//! `alloc_ptr`/`alloc_limit` now point into a region that the heap will
//! hand out as a fresh chunk to the next allocator. Skipping invalidation
//! either panics on out-of-bounds writes or corrupts another VM's chunk.

use std::{
    collections::HashMap,
    sync::{Arc, atomic::AtomicBool},
};

use baml_tests::engine::compile_source;
use bex_vm::BexVm;
use bex_vm_types::RootHaver;

fn make_vm() -> BexVm {
    // The program contents don't matter — we only need a VM with a live
    // TLAB. `from_program` calls `Tlab::new`, which reserves a chunk.
    let program = compile_source("function noop() -> int { 0 }");
    BexVm::from_program(program, Arc::new(AtomicBool::new(false))).expect("from_program")
}

#[test]
fn forward_roots_invalidates_tlab() {
    let mut vm = make_vm();
    assert!(vm.tlab.is_valid(), "fresh TLAB should hold a chunk");

    vm.forward_roots(&HashMap::new());
    assert!(
        !vm.tlab.is_valid(),
        "TLAB must be invalidated by forward_roots so the next alloc refills"
    );
}
