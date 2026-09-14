//! Unit tests for Phase 8: `Instance::class_type_args` and method-frame seeding.
//!
//! These tests exercise:
//! - `Object::Instance` carrying `class_type_args` (8.1)
//! - `AllocInstance { ntypeargs }` populating `class_type_args` from the stack (8.2)
//! - A method-like function body reading `TypeArgRef(0)` when the frame has
//!   been seeded with `[RuntimeTy::int()]` (simulating the 8.5 path) (8.5)
//!
//! The tests inject synthetic bytecode directly into a compiled `Program`
//! (produced by `compile_source`) so no compiler pipeline is required.

use std::sync::{Arc, atomic::AtomicBool};

use baml_db::testing::compile_source;
use baml_type::{Name, RealizedTy, TyAttr, TyTemplate, TypeName};
use bex_vm::{BexVm, VmExecState};
use bex_vm_types::{
    ConstValue, FunctionCaptureProps, Instruction, Object, ObjectIndex, Value,
    bytecode::Bytecode,
    types::{Class, Function, FunctionKind, FunctionOrigin, Program},
};

/// Minimal valid BAML source — provides error/panic class objects.
const STUB_SOURCE: &str = r#"
function _stub() -> int { 42 }
"#;

/// Add a synthetic bytecode function to an existing `Program`.
fn inject_function(
    program: &mut Program,
    fn_name: &str,
    arity: usize,
    instructions: Vec<Instruction>,
    constants: Vec<ConstValue>,
) -> usize {
    let bytecode = Bytecode {
        instructions,
        constants,
        ..Bytecode::default()
    };
    let func = Function {
        name: fn_name.to_string(),
        source_file: String::new(),
        docstring: None,
        declared_name: None,
        arity,
        real_local_count: 0,
        bytecode,
        kind: FunctionKind::Bytecode,
        local_names: vec![],
        debug_locals: vec![],
        span: baml_type::Span::fake(),
        return_type: baml_type::TyTemplate::Int {
            attr: baml_type::TyAttr::default(),
        },
        param_names: vec![],
        param_types: vec![],
        param_has_default: vec![false; arity],
        display_type_params: vec![],
        generic_param_bounds: vec![],
        display_param_types: vec![],
        display_return_type: "int".to_string(),
        throws_type: baml_type::TyTemplate::Never {
            attr: baml_type::TyAttr::default(),
        },
        origin: FunctionOrigin::UserDefined,
        is_interface_body: false,
        native_key: None,
        body_meta: None,
        capture: FunctionCaptureProps::disabled(),
        function_id: 0,
        runtime_package: bex_vm_types::HeapPtr::null(),
    };
    let fn_obj_idx = program.add_object(Object::Function(Box::new(func)));
    let global_slot = program.globals.len();
    program
        .function_indices
        .insert(fn_name.to_string(), fn_obj_idx);
    program.add_global(ConstValue::Object(ObjectIndex::from_raw(fn_obj_idx)));
    program
        .function_global_indices
        .insert(fn_name.to_string(), global_slot);
    fn_obj_idx
}

/// Run a named function to completion and return the result + VM.
fn run_fn(program: Program, fn_name: &str) -> (Value, BexVm) {
    let function_index = program.function_index(fn_name).unwrap_or_else(|| {
        panic!("function {fn_name:?} not found");
    });
    let mut vm =
        BexVm::from_program(program, Arc::new(AtomicBool::new(false))).expect("from_program");
    let function_ptr = vm.heap.compile_time_ptr(function_index);
    vm.set_entry_point(function_ptr, &[]);
    loop {
        match vm.exec().expect("exec") {
            VmExecState::Complete(v) => return (v, vm),
            VmExecState::EarlyYield => continue,
            other => panic!("unexpected VM state: {other:?}"),
        }
    }
}

// ─── 8.1 + 8.2 ── AllocInstance with ntypeargs stores class_type_args ────────

/// `AllocInstance { class_obj, ntypeargs: 1 }` should pop the top `Object::Type`
/// and store it in the resulting `Instance::class_type_args`.
#[test]
fn alloc_instance_ntypeargs_stores_class_type_args() {
    let mut program = compile_source(STUB_SOURCE);

    // Add a synthetic class to the object pool.
    let class_ptr_idx = program.add_object(Object::Class(Box::new(Class {
        name: bex_vm_types::DeclarationName::Declared(TypeName::local(Name::new("TestBox"))),
        fields: vec![],
        description: None,
        alias: None,
        docstring: None,
        other: indexmap::IndexMap::new(),
        type_tag: baml_type::typetag::TypeTag::from_i64(100),
        ty_attr: TyAttr::default(),
        has_cleanup: false,
        generic_param_count: 0,
        owner: bex_vm_types::HeapPtr::null(),
    })));

    // Function: push RuntimeTy::int() as a type arg, then AllocInstance with ntypeargs=1.
    let fn_name = "user.test_alloc_typearg";
    inject_function(
        &mut program,
        fn_name,
        0,
        vec![
            Instruction::LoadType(0),
            Instruction::AllocInstance {
                class_obj: ObjectIndex::from_raw(class_ptr_idx),
                ntypeargs: 1,
            },
            Instruction::Return,
        ],
        vec![ConstValue::Type(TyTemplate::from(
            baml_type::RealizedTy::int(),
        ))],
    );

    let (result, vm) = run_fn(program, fn_name);

    let Some(inst_ptr) = result.as_object_ptr() else {
        panic!("expected Object, got {result:?}");
    };
    match vm.get_object(inst_ptr) {
        Object::Instance(inst) => {
            assert_eq!(
                inst.class_type_args.to_vec(),
                vec![RealizedTy::int()],
                "Instance::class_type_args should equal [RealizedTy::int()]"
            );
        }
        other => panic!("expected Object::Instance, got {other:?}"),
    }
}

/// `AllocInstance { class_obj, ntypeargs: 0 }` should leave `class_type_args` empty.
#[test]
fn alloc_instance_ntypeargs_zero_gives_empty_class_type_args() {
    let mut program = compile_source(STUB_SOURCE);

    let class_ptr_idx = program.add_object(Object::Class(Box::new(Class {
        name: bex_vm_types::DeclarationName::Declared(TypeName::local(Name::new("MonoClass"))),
        fields: vec![],
        description: None,
        alias: None,
        docstring: None,
        other: indexmap::IndexMap::new(),
        type_tag: baml_type::typetag::TypeTag::from_i64(101),
        ty_attr: TyAttr::default(),
        has_cleanup: false,
        generic_param_count: 0,
        owner: bex_vm_types::HeapPtr::null(),
    })));

    let fn_name = "user.test_mono_alloc";
    inject_function(
        &mut program,
        fn_name,
        0,
        vec![
            Instruction::AllocInstance {
                class_obj: ObjectIndex::from_raw(class_ptr_idx),
                ntypeargs: 0,
            },
            Instruction::Return,
        ],
        vec![],
    );

    let (result, vm) = run_fn(program, fn_name);
    let Some(inst_ptr) = result.as_object_ptr() else {
        panic!("expected Object, got {result:?}");
    };
    match vm.get_object(inst_ptr) {
        Object::Instance(inst) => {
            assert!(
                inst.class_type_args.is_empty(),
                "monomorphic Instance::class_type_args should be empty"
            );
        }
        other => panic!("expected Object::Instance, got {other:?}"),
    }
}

// ─── 8.5 ── frame.type_args seeded from receiver class_type_args ─────────────

/// Manually seed `frame.type_args = [RuntimeTy::int()]` on a method frame and verify
/// that `LoadType(TypeArgRef(0))` returns `Object::Type(int)`.
///
/// This simulates what `execute_call_from_locals_offset` does in Phase 8.5
/// when a `BoundMethod` callee has a receiver with `class_type_args = [RuntimeTy::int()]`.
#[test]
fn method_frame_type_args_seeded_with_class_type_args() {
    let mut program = compile_source(STUB_SOURCE);

    // Method body: LoadType(TypeArgRef(0)) + Return.
    let fn_name = "user.method_body";
    inject_function(
        &mut program,
        fn_name,
        0,
        vec![Instruction::LoadType(0), Instruction::Return],
        vec![ConstValue::Type(TyTemplate::TypeArgRef(0))],
    );

    let function_index = program.function_index(fn_name).expect("fn not found");
    let mut vm =
        BexVm::from_program(program, Arc::new(AtomicBool::new(false))).expect("from_program");
    let function_ptr = vm.heap.compile_time_ptr(function_index);
    vm.set_entry_point(function_ptr, &[]);

    // Simulate 8.5: seed frame.type_args with the receiver's class_type_args.
    {
        use bex_vm::Frame;
        let frame = vm.frames.last_mut().expect("entry frame");
        if let Frame::Bytecode(bf) = frame {
            bf.type_args = vec![RealizedTy::int()]; // receiver.class_type_args = [int]
        } else {
            panic!("entry frame should be Bytecode");
        }
    }

    let result = loop {
        match vm.exec().expect("exec") {
            VmExecState::Complete(v) => break v,
            VmExecState::EarlyYield => continue,
            other => panic!("unexpected: {other:?}"),
        }
    };

    let Some(ptr) = result.as_object_ptr() else {
        panic!("expected Object, got {result:?}");
    };
    match vm.get_object(ptr) {
        Object::Type(type_value) => {
            assert_eq!(
                type_value.ty,
                RealizedTy::int(),
                "TypeArgRef(0) with class_type_args=[int] should yield int"
            );
        }
        other => panic!("expected Object::Type, got {other:?}"),
    }
}

/// Seed `frame.type_args = [RuntimeTy::string()]` and confirm `TypeArgRef(0)` yields string.
#[test]
fn method_frame_type_args_seeded_string() {
    let mut program = compile_source(STUB_SOURCE);
    let fn_name = "user.method_body_str";
    inject_function(
        &mut program,
        fn_name,
        0,
        vec![Instruction::LoadType(0), Instruction::Return],
        vec![ConstValue::Type(TyTemplate::TypeArgRef(0))],
    );

    let function_index = program.function_index(fn_name).expect("fn not found");
    let mut vm =
        BexVm::from_program(program, Arc::new(AtomicBool::new(false))).expect("from_program");
    let function_ptr = vm.heap.compile_time_ptr(function_index);
    vm.set_entry_point(function_ptr, &[]);

    {
        use bex_vm::Frame;
        let frame = vm.frames.last_mut().expect("entry frame");
        if let Frame::Bytecode(bf) = frame {
            bf.type_args = vec![RealizedTy::string()];
        }
    }

    let result = loop {
        match vm.exec().expect("exec") {
            VmExecState::Complete(v) => break v,
            VmExecState::EarlyYield => continue,
            other => panic!("unexpected: {other:?}"),
        }
    };

    let Some(ptr) = result.as_object_ptr() else {
        panic!("expected Object")
    };
    match vm.get_object(ptr) {
        Object::Type(type_value) => {
            assert_eq!(
                type_value.ty,
                RealizedTy::string(),
                "TypeArgRef(0) with class_type_args=[string] should yield string"
            );
        }
        other => panic!("expected Object::Type, got {other:?}"),
    }
}

// ─── Regression ── `map`/`flat_map` tag the result with `U`, not the receiver `T`

/// `[int].map(x -> string)` must stamp the result array's runtime `element_ty`
/// with the closure return type `U` (= `string`), NOT the receiver element type
/// `T` (= `int`). MIR prepends the receiver's class type args ahead of the
/// method's own `<U, E>`, so the frame's type args are `[T, U, E]`; the result
/// element type is the second-to-last (`U`), not `.first()` (`T`).
#[test]
fn map_result_element_ty_is_closure_return_not_receiver() {
    let program = compile_source(
        r#"
function f() -> string[] {
    [1, 2].map((x: int) -> string { "s" })
}
"#,
    );
    let (result, vm) = run_fn(program, "user.f");
    let ptr = result
        .as_object_ptr()
        .expect("map result should be an array object");
    match vm.get_object(ptr) {
        Object::Array(arr) => assert!(
            matches!(&*arr.element_ty, RealizedTy::String { .. }),
            "map result element_ty should be `string` (closure return `U`), not `{:?}` \
             (the receiver `T`)",
            arr.element_ty
        ),
        other => panic!("expected Object::Array result, got {other:?}"),
    }
}
