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

use baml_project::testing::compile_source;
use bex_vm::BexVm;
use bex_vm_types::{HeapPtr, RootHaver, Value};

fn make_vm() -> BexVm {
    // The program contents don't matter — we only need a VM whose TLAB
    // can be materialized on demand. `BexVm::new` builds the TLAB with
    // `Tlab::new_empty` (lazy refill) so the chunk is reserved on the
    // first allocation under the VM's permit, not at construction time.
    let program = compile_source("function noop() -> int { 0 }");
    BexVm::from_program(program, Arc::new(AtomicBool::new(false))).expect("from_program")
}

#[test]
fn forward_roots_invalidates_tlab() {
    let mut vm = make_vm();
    // `Tlab::new_empty` defers chunk reservation until first alloc, so a
    // fresh VM has an invalid TLAB by construction. Force a refill by
    // allocating once, then verify `forward_roots` invalidates it.
    let _ = vm.tlab.alloc_string("force_refill".to_string());
    assert!(
        vm.tlab.is_valid(),
        "TLAB should hold a chunk after the first allocation"
    );

    vm.forward_roots(&HashMap::new());
    assert!(
        !vm.tlab.is_valid(),
        "TLAB must be invalidated by forward_roots so the next alloc refills"
    );
}

#[test]
fn forward_roots_remaps_stack_object_pointers() {
    let mut vm = make_vm();

    let old_ptr = vm.tlab.alloc_string("before_gc".to_string());
    let new_ptr = vm.tlab.alloc_string("after_gc".to_string());
    let untouched_ptr = vm.tlab.alloc_string("stays_put".to_string());

    vm.stack.0.push(Value::Object(old_ptr));
    vm.stack.0.push(Value::Int(42));
    vm.stack.0.push(Value::Object(untouched_ptr));

    let mut forwarding: HashMap<HeapPtr, HeapPtr> = HashMap::new();
    forwarding.insert(old_ptr, new_ptr);

    vm.forward_roots(&forwarding);

    assert_eq!(
        vm.stack.0[0],
        Value::Object(new_ptr),
        "stack slot 0 should have been rewritten to the forwarded pointer"
    );
    assert_eq!(
        vm.stack.0[1],
        Value::Int(42),
        "non-object stack values must be left alone"
    );
    assert_eq!(
        vm.stack.0[2],
        Value::Object(untouched_ptr),
        "pointers without a forwarding entry must be left alone"
    );
}
