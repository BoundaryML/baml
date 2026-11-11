//! Tests for `<BexVm as RootHaver>::forward_roots`.
//!
//! The engine's GC coordinator calls `forward_roots` on every parked VM
//! after a collection. The collector does not move objects, so the hook's
//! work is to drop weak references to the dead and to tell the VM's TLAB a
//! collection ran; the TLAB's current blocks stay valid, since nothing else
//! can be handed a block it owns.

use std::{
    collections::HashMap,
    sync::{Arc, atomic::AtomicBool},
};

use baml_test_support::compile_source;
use bex_vm::BexVm;
use bex_vm_types::{ForwardRoots, HeapPtr, NothingCollected, RootHaverExt, Value};

fn make_vm() -> BexVm {
    // The program contents don't matter — we only need a VM whose TLAB
    // can be materialized on demand. `BexVm::new` builds the TLAB with
    // `Tlab::new_empty` (lazy refill) so the chunk is reserved on the
    // first allocation under the VM's permit, not at construction time.
    let program = compile_source("function noop() -> int { 0 }");
    BexVm::from_program(program, Arc::new(AtomicBool::new(false))).expect("from_program")
}

#[test]
fn forward_roots_keeps_the_tlab_blocks() {
    let mut vm = make_vm();
    // `Tlab::new_empty` defers taking a block until first alloc. A float is
    // an `Object` cell, the class `is_valid` looks at.
    let _ = vm.tlab.alloc_float(1.0);
    assert!(
        vm.tlab.is_valid(),
        "TLAB should hold a block after the first allocation"
    );
    let remaining = vm.tlab.remaining();

    vm.forward_roots(&mut ForwardRoots::new(&HashMap::new(), &NothingCollected));
    assert!(vm.tlab.is_valid(), "objects never move, so the block stays");
    assert_eq!(vm.tlab.remaining(), remaining);
}

#[test]
fn forward_roots_remaps_stack_object_pointers() {
    let mut vm = make_vm();

    let old_ptr = vm.tlab.alloc_string("before_gc".to_string());
    let new_ptr = vm.tlab.alloc_string("after_gc".to_string());
    let untouched_ptr = vm.tlab.alloc_string("stays_put".to_string());

    vm.stack.try_push(Value::object(old_ptr)).unwrap();
    vm.stack.try_push(Value::int(42)).unwrap();
    vm.stack.try_push(Value::object(untouched_ptr)).unwrap();

    let mut forwarding: HashMap<HeapPtr, HeapPtr> = HashMap::new();
    forwarding.insert(old_ptr, new_ptr);

    vm.forward_roots(&mut ForwardRoots::new(&forwarding, &NothingCollected));

    assert_eq!(
        vm.stack[bex_vm_types::StackIndex::from_raw(0)],
        Value::object(new_ptr),
        "stack slot 0 should have been rewritten to the forwarded pointer"
    );
    assert_eq!(
        vm.stack[bex_vm_types::StackIndex::from_raw(1)],
        Value::int(42),
        "non-object stack values must be left alone"
    );
    assert_eq!(
        vm.stack[bex_vm_types::StackIndex::from_raw(2)],
        Value::object(untouched_ptr),
        "pointers without a forwarding entry must be left alone"
    );
}
