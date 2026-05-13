//! Tests verifying that `Object::Function` allocated into gen0 — as will
//! happen when `baml.runtime.compile` returns a compiled lambda — is handled
//! correctly by the GC root-collection and forwarding passes.
//!
//! Specifically:
//!
//! - `BexVm::collect_frame_roots` must include a gen0 function pointer held
//!   by a bytecode frame.
//! - `BexVm::apply_frame_forwarding` must update the frame's `function`
//!   field when the GC moves the object.
//! - A full `BexHeap::collect_garbage` cycle that includes the frame root
//!   must forward the frame ptr and leave a valid `Object::Function` at the
//!   new location.

// This test file exercises unsafe GC internals directly, mirroring the
// pattern used by bex_heap's own test suite (collect_garbage, HeapPtr::get).
#![allow(unsafe_code)]

use std::sync::{Arc, atomic::AtomicBool};

use baml_project::testing::compile_source;
use baml_type::{Span, Ty, TyAttr};
use bex_vm::{BexVm, Frame};
use bex_vm_types::{
    Bytecode, Function, FunctionKind, FunctionOrigin, HeapPtr, Object, RootHaver, Value,
};

fn make_vm() -> BexVm {
    let program = compile_source("function noop() -> int { 0 }");
    BexVm::from_program(program, Arc::new(AtomicBool::new(false))).expect("from_program")
}

/// Build a minimal `Function` that can be allocated into gen0.
///
/// Empty bytecode, arity 0, 0 locals — enough for `set_entry_point` to push
/// a valid `BytecodeFrame`.
fn make_minimal_function() -> Function {
    Function {
        name: "gen0_lambda".to_string(),
        source_file: String::new(),
        arity: 0,
        real_local_count: 0,
        bytecode: Bytecode::default(),
        kind: FunctionKind::Bytecode,
        local_names: vec![],
        debug_locals: vec![],
        span: Span::fake(),
        block_notifications: vec![],
        viz_nodes: vec![],
        return_type: Ty::int(),
        stream_return_type: Ty::Null {
            attr: TyAttr::default(),
        },
        param_names: vec![],
        param_types: vec![],
        param_has_default: vec![],
        throws_type: None,
        origin: FunctionOrigin::UserDefined,
        body_meta: None,
        trace: false,
        aux_object_ptrs: Vec::new(),
    }
}

/// `collect_frame_roots` must include a gen0 function pointer that was pushed
/// onto the call stack via `set_entry_point`.
#[test]
fn collect_frame_roots_includes_gen0_function() {
    let mut vm = make_vm();

    // Allocate a Function into gen0 (not compile_time).
    let fn_ptr = vm
        .tlab
        .alloc(Object::Function(Box::new(make_minimal_function())));
    assert!(
        !vm.heap.is_compile_time_ptr(fn_ptr),
        "freshly TLAB-allocated function must not be in compile_time region"
    );

    // Push a bytecode frame referencing the gen0 function.
    vm.set_entry_point(fn_ptr, &[]);

    // The frame root-set must include our gen0 function pointer.
    let frame_roots = vm.collect_frame_roots();
    assert!(
        frame_roots.contains(&fn_ptr),
        "collect_frame_roots must report the gen0 function pointer; \
         roots = {frame_roots:?}"
    );
}

/// `apply_frame_forwarding` must rewrite the frame's `function` field when
/// the GC moves the gen0 function object to a new address.
#[test]
fn apply_frame_forwarding_updates_gen0_function_ptr() {
    let mut vm = make_vm();

    let fn_ptr = vm
        .tlab
        .alloc(Object::Function(Box::new(make_minimal_function())));
    vm.set_entry_point(fn_ptr, &[]);

    // Simulate GC moving the object: allocate a second (distinct) object to
    // stand in as the "new location", then build a forwarding map.
    let moved_ptr = vm
        .tlab
        .alloc(Object::Function(Box::new(make_minimal_function())));
    assert_ne!(
        fn_ptr, moved_ptr,
        "two TLAB allocations must produce distinct pointers"
    );

    let mut forwarding = std::collections::HashMap::new();
    forwarding.insert(fn_ptr, moved_ptr);

    // Apply the forwarding map to all frame function pointers.
    vm.apply_frame_forwarding(&forwarding);

    // The frame's function pointer must now be the new (forwarded) address.
    let Frame::Bytecode(bf) = vm.frames.last().expect("frame must exist") else {
        panic!("expected Bytecode frame");
    };
    assert_eq!(
        bf.function, moved_ptr,
        "frame function pointer must be updated to the forwarded address"
    );
}

/// A full GC cycle must correctly forward a gen0 `Object::Function` that is
/// held alive by a VM frame root, and the forwarded pointer must still
/// reference a valid `Object::Function`.
#[test]
fn gen0_function_in_flight_gc() {
    let mut vm = make_vm();

    // Allocate a gen0 Function and push a frame referencing it.
    let fn_ptr = vm
        .tlab
        .alloc(Object::Function(Box::new(make_minimal_function())));
    vm.set_entry_point(fn_ptr, &[]);

    // Collect all roots: frame ptrs + stack objects.
    let mut all_roots = vm.collect_frame_roots();
    let stack_roots: Vec<HeapPtr> = vm
        .stack
        .0
        .iter()
        .filter_map(|v| match v {
            Value::Object(ptr) => Some(*ptr),
            _ => None,
        })
        .collect();
    all_roots.extend(stack_roots);

    // Run a full major GC with the collected roots.
    // SAFETY: No other VMs are running; this is a single-threaded test.
    let (_stats, _remapped, forwarding) = unsafe { vm.heap.collect_garbage(&all_roots) };

    // The GC must have forwarded our function pointer (it was a root).
    let new_fn_ptr = forwarding
        .get(&fn_ptr)
        .copied()
        .expect("gen0 function must be in the forwarding map (it was a live root)");

    // `forward_roots` applies forwarding to both frames and stack, and also
    // invalidates the TLAB so the next allocation refills from the post-GC cursor.
    vm.forward_roots(&forwarding);

    // The frame's function pointer must now be the new (forwarded) address.
    let Frame::Bytecode(bf) = vm.frames.last().expect("frame must exist") else {
        panic!("expected Bytecode frame");
    };
    assert_eq!(
        bf.function, new_fn_ptr,
        "frame function pointer must be updated to the post-GC address"
    );

    // The new pointer must still reference a valid Object::Function.
    // SAFETY: GC has completed; new_fn_ptr is valid in the post-GC (Gen2) heap.
    let obj = unsafe { new_fn_ptr.get() };
    assert!(
        matches!(obj, Object::Function(_)),
        "forwarded pointer must still reference an Object::Function, got {obj:?}"
    );
}
