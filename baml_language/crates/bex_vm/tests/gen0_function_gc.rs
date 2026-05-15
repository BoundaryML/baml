//! Tests verifying that `Object::Function` allocated into gen0 — as happens
//! when `reflect.Package.add_compile` lifts a freshly-compiled function into
//! the heap — is handled correctly by the GC root-collection and forwarding
//! passes.
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
    Bytecode, Function, FunctionKind, FunctionOrigin, HeapPtr, Object, Package, PackageGlobals,
    RootHaver, Value,
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
        package_name: String::new(),
        package: HeapPtr::null(),
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

/// `Object::Function.bytecode.resolved_constants` entries must participate in
/// the GC root walk. Runtime-lifted functions allocate gen0 strings (and
/// reference gen0 function `HeapPtr`s) here; if the GC doesn't trace them,
/// they get reclaimed out from under the function on the next collection.
///
/// Setup: build a gen0 function whose `resolved_constants[0]` is a
/// `Value::Object(<gen0 string>)`. Root only the function (not the string
/// directly). Run a major GC. Expect:
/// 1. The string `HeapPtr` is in the forwarding map (proves it was reached
///    transitively through the function's `resolved_constants` walk).
/// 2. After applying the forwarding map to the function's bytecode, the
///    string `HeapPtr` is updated to the new (post-GC) address.
/// 3. The new pointer still references a valid `Object::String`.
#[test]
fn gen0_function_resolved_constants_traced_by_gc() {
    let mut vm = make_vm();

    // Allocate a gen0 string we want to keep alive only via the function's
    // resolved_constants slot.
    let str_ptr = vm.tlab.alloc(Object::String("greeting".to_string()));
    assert!(
        !vm.heap.is_compile_time_ptr(str_ptr),
        "freshly TLAB-allocated string must not be in compile_time region"
    );

    // Build a function whose bytecode has the string in resolved_constants.
    let mut func = make_minimal_function();
    func.bytecode.constants = vec![]; // `constants` doesn't carry runtime refs here
    func.bytecode.resolved_constants = vec![Value::Object(str_ptr)];
    let fn_ptr = vm.tlab.alloc(Object::Function(Box::new(func)));

    // Push a frame so the function is rooted but the string is NOT (it's only
    // reachable through `function.bytecode.resolved_constants`).
    vm.set_entry_point(fn_ptr, &[]);

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
    // Sanity: the string is NOT a direct root.
    assert!(
        !all_roots.contains(&str_ptr),
        "the string should only be reachable via function.resolved_constants"
    );

    // SAFETY: single-threaded test, no other VMs running.
    let (_stats, _remapped, forwarding) = unsafe { vm.heap.collect_garbage(&all_roots) };

    // The string must have been reached and forwarded.
    let new_str_ptr = forwarding.get(&str_ptr).copied().expect(
        "gen0 string in function.resolved_constants must be in the forwarding map \
         (reached transitively through the function root)",
    );

    vm.forward_roots(&forwarding);

    // The function's resolved_constants[0] should be updated to point at the
    // new string location.
    let new_fn_ptr = forwarding
        .get(&fn_ptr)
        .copied()
        .expect("function must be in forwarding map");
    // SAFETY: GC has completed; new_fn_ptr is valid in the post-GC heap.
    let Object::Function(post_func) = (unsafe { new_fn_ptr.get() }) else {
        panic!("forwarded pointer must reference Object::Function");
    };
    match post_func.bytecode.resolved_constants.first() {
        Some(Value::Object(p)) => assert_eq!(
            *p, new_str_ptr,
            "resolved_constants[0] must be forwarded to the post-GC string address"
        ),
        other => panic!("expected resolved_constants[0] = Value::Object, got {other:?}"),
    }

    // SAFETY: GC complete; new_str_ptr is valid.
    let str_obj = unsafe { new_str_ptr.get() };
    match str_obj {
        Object::String(s) => assert_eq!(s, "greeting", "string contents must survive GC"),
        other => panic!("forwarded string ptr must reference Object::String, got {other:?}"),
    }
}

/// An `Object::Function` allocated into gen0 with `function.package`
/// pointing at a gen0 `Object::Package` must keep the package alive across
/// GC. This is the failure mode where a user holds a runtime function
/// returned by `pkg.get<F>(...)` after dropping the package wrapper — if
/// the Function GC arm doesn't walk `f.package`, the package gets reclaimed
/// and the next call dereferences a dangling pointer.
#[test]
fn gen0_function_package_traced_by_gc() {
    let mut vm = make_vm();

    // Allocate a gen0 Package we want to keep alive only via function.package.
    let pkg = Package {
        name: "_pkg_test".to_string(),
        items: std::collections::HashMap::new(),
        globals: PackageGlobals::Dynamic(Vec::new()),
        function_slot_map: std::collections::HashMap::new(),
        eval_counter: 0,
    };
    let pkg_ptr = vm.tlab.alloc(Object::Package(Box::new(pkg)));
    assert!(
        !vm.heap.is_compile_time_ptr(pkg_ptr),
        "freshly TLAB-allocated package must not be in compile_time region"
    );

    // Build a function whose `package` points at the gen0 Package.
    let mut func = make_minimal_function();
    func.package = pkg_ptr;
    let fn_ptr = vm.tlab.alloc(Object::Function(Box::new(func)));

    // Push a frame so the function is rooted but the package is NOT
    // (only reachable through `function.package`).
    vm.set_entry_point(fn_ptr, &[]);

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
    assert!(
        !all_roots.contains(&pkg_ptr),
        "the package should only be reachable via function.package"
    );

    // SAFETY: single-threaded test, no other VMs running.
    let (_stats, _remapped, forwarding) = unsafe { vm.heap.collect_garbage(&all_roots) };

    // The package must have been reached and forwarded.
    let new_pkg_ptr = forwarding.get(&pkg_ptr).copied().expect(
        "gen0 package referenced via function.package must be in the forwarding map \
         (reached transitively through the function root)",
    );

    vm.forward_roots(&forwarding);

    // The function's `package` field should be updated to the new address.
    let new_fn_ptr = forwarding
        .get(&fn_ptr)
        .copied()
        .expect("function must be in forwarding map");
    // SAFETY: GC has completed; new_fn_ptr is valid in the post-GC heap.
    let Object::Function(post_func) = (unsafe { new_fn_ptr.get() }) else {
        panic!("forwarded pointer must reference Object::Function");
    };
    assert_eq!(
        post_func.package, new_pkg_ptr,
        "function.package must be forwarded to the post-GC package address"
    );

    // The new package address must still hold an Object::Package with our test name.
    // SAFETY: GC complete; new_pkg_ptr is valid.
    let pkg_obj = unsafe { new_pkg_ptr.get() };
    match pkg_obj {
        Object::Package(p) => assert_eq!(p.name, "_pkg_test", "package contents must survive GC"),
        other => panic!("forwarded package ptr must reference Object::Package, got {other:?}"),
    }
}
