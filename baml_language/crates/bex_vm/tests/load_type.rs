//! Unit tests for the `LoadType` instruction and the `type_args` calling convention.
//!
//! These tests exercise Phase 3 of the type-reflection implementation:
//! - `TyTemplate::from(...)` → `LoadType` materialises the concrete `RuntimeTy`
//! - `TyTemplate::TypeArgRef(0)` → `LoadType` substitutes from `frame.type_args[0]`
//! - Composite templates (e.g. `Array(TypeArgRef(0))`) substitute correctly
//! - `Call { ntypeargs }` pops type args from the stack and stores them in the frame
//!
//! The tests inject synthetic bytecode functions into a fully-compiled `Program`
//! (produced by `compile_source`) so that the error/panic class objects required
//! by `BexVm::new` are always present.

use std::sync::{Arc, atomic::AtomicBool};

use baml_project::testing::compile_source;
use baml_type::{RealizedTy, TyTemplate};
use bex_vm::{BexVm, VmExecState};
use bex_vm_types::{
    ConstValue, FunctionCaptureProps, GlobalIndex, Instruction, Object, ObjectIndex, Value,
    bytecode::Bytecode,
    types::{Function, FunctionKind, FunctionOrigin, MintId, Program},
};

/// Minimal valid BAML source used as the base for all tests.
/// It provides the error/panic class objects that `BexVm::new` requires.
const STUB_SOURCE: &str = r#"
function _stub() -> int {
    42
}
"#;

/// Add a synthetic bytecode function to an existing `Program` and return
/// `(program, function_name, function_object_index)`.
///
/// The function has arity 0 and no locals.  Its bytecode is `instructions`
/// with constant pool `constants`.
fn inject_function(
    program: &mut Program,
    fn_name: &str,
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
        arity: 0,
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
        param_has_default: vec![],
        display_type_params: vec![],
        generic_param_bounds: vec![],
        display_param_types: vec![],
        display_return_type: "int".to_string(),
        throws_type: baml_type::TyTemplate::Never {
            attr: baml_type::TyAttr::default(),
        },
        origin: FunctionOrigin::UserDefined,
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

/// Compile a base program, inject the test function, and run to completion.
/// Returns both the result `Value` and the VM so tests can inspect
/// heap-allocated objects (e.g. `Object::Type`) referenced by the result.
fn run_with_bytecode_keep_vm(
    fn_name: &str,
    instructions: Vec<Instruction>,
    constants: Vec<ConstValue>,
) -> (Value, BexVm) {
    let mut program = compile_source(STUB_SOURCE);
    inject_function(&mut program, fn_name, instructions, constants);

    let function_index = program
        .function_index(fn_name)
        .unwrap_or_else(|| panic!("function {fn_name:?} not found"));

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

// ─── 3.1 & 3.5 ── LoadType with a fully-concrete template ───────────────────

/// `LoadType(k)` where `k` is a `ConstValue::Type(TyTemplate::from(int))`
/// should push an `Object::Type` whose `TypeValue` carries `RealizedTy::int()`
/// and a deterministic static mint.
#[test]
fn load_type_concrete_int() {
    let template = TyTemplate::from(baml_type::RealizedTy::int());
    let (result, vm) = run_with_bytecode_keep_vm(
        "user.test_load_int",
        vec![Instruction::LoadType(0), Instruction::Return],
        vec![ConstValue::Type(template)],
    );

    let Some(ptr) = result.as_object_ptr() else {
        panic!("expected Object, got {result:?}");
    };
    match vm.get_object(ptr) {
        Object::Type(type_value) => {
            assert_eq!(
                type_value.ty,
                RealizedTy::int(),
                "LoadType(int) should materialise RealizedTy::int"
            );
            assert!(matches!(type_value.mint(), MintId::Static(_)));
        }
        other => panic!("expected Object::Type, got {other:?}"),
    }
}

/// A `TyTemplate::from(string)` produces a `RuntimeTy::string()` payload distinct
/// from a `TyTemplate::from(int)`, and the resulting heap objects compare
/// unequal under `==`.
#[test]
fn load_type_concrete_string_different_from_int() {
    let (r_int, vm_int) = run_with_bytecode_keep_vm(
        "user.test_load_int2",
        vec![Instruction::LoadType(0), Instruction::Return],
        vec![ConstValue::Type(TyTemplate::from(
            baml_type::RealizedTy::int(),
        ))],
    );
    let (r_str, vm_str) = run_with_bytecode_keep_vm(
        "user.test_load_str",
        vec![Instruction::LoadType(0), Instruction::Return],
        vec![ConstValue::Type(TyTemplate::from(
            baml_type::RealizedTy::string(),
        ))],
    );

    let Some(p_int) = r_int.as_object_ptr() else {
        panic!("expected Object for int, got {r_int:?}");
    };
    let Some(p_str) = r_str.as_object_ptr() else {
        panic!("expected Object for string, got {r_str:?}");
    };

    let int_ty = match vm_int.get_object(p_int) {
        Object::Type(type_value) => type_value.ty.clone(),
        other => panic!("expected Object::Type for int, got {other:?}"),
    };
    let str_ty = match vm_str.get_object(p_str) {
        Object::Type(type_value) => type_value.ty.clone(),
        other => panic!("expected Object::Type for string, got {other:?}"),
    };

    assert_eq!(int_ty, RealizedTy::int());
    assert_eq!(str_ty, RealizedTy::string());
    assert_ne!(
        int_ty, str_ty,
        "int and string LoadType payloads must differ"
    );
}

// ─── 3.5 ── LoadType with TypeArgRef — type_args set directly on the frame ──

/// Set `frame.type_args = [RuntimeTy::string()]` externally and run
/// `LoadType(TypeArgRef(0))` — result should be `Object::Type(string)`.
///
/// This test verifies the `TypeArgRef` substitution path without going through
/// the full `Call { ntypeargs }` flow.
#[test]
fn load_type_type_arg_ref_substitutes_from_frame() {
    let template = TyTemplate::TypeArgRef(0);
    let fn_name = "user.test_typeargref";

    let mut program = compile_source(STUB_SOURCE);
    let fn_obj_idx = inject_function(
        &mut program,
        fn_name,
        vec![Instruction::LoadType(0), Instruction::Return],
        vec![ConstValue::Type(template)],
    );

    let function_index = program.function_index(fn_name).expect("function not found");
    assert_eq!(function_index, fn_obj_idx);

    let mut vm =
        BexVm::from_program(program, Arc::new(AtomicBool::new(false))).expect("from_program");
    let function_ptr = vm.heap.compile_time_ptr(function_index);
    vm.set_entry_point(function_ptr, &[]);

    // Directly inject type_args into the entry-point frame (slot 0).
    {
        use bex_vm::Frame;
        let frame = vm.frames.last_mut().expect("entry frame must exist");
        if let Frame::Bytecode(bf) = frame {
            bf.type_args = vec![RealizedTy::string()];
        } else {
            panic!("entry frame should be Bytecode");
        }
    }

    let result = loop {
        match vm.exec().expect("exec") {
            VmExecState::Complete(v) => break v,
            VmExecState::EarlyYield => continue,
            other => panic!("unexpected state: {other:?}"),
        }
    };

    // The result must be an Object::Type wrapping RealizedTy::string()
    let Some(ptr) = result.as_object_ptr() else {
        panic!("expected Object, got {result:?}");
    };
    let obj = vm.get_object(ptr);
    match obj {
        Object::Type(type_value) => {
            assert_eq!(
                type_value.ty,
                RealizedTy::string(),
                "TypeArgRef(0) should resolve to string"
            );
        }
        other => panic!("expected Object::Type, got {other:?}"),
    }
}

// ─── 3.5 ── Composite template Array(TypeArgRef(0)) ─────────────────────────

/// `TyTemplate::Array(TypeArgRef(0))` with `frame.type_args[0] = RuntimeTy::int()`
/// should produce an `Object::Type` whose `TypeValue` carries
/// `RealizedTy::list(int)`.
#[test]
fn load_type_array_of_type_arg_ref() {
    let template = TyTemplate::list(TyTemplate::TypeArgRef(0));
    let fn_name = "user.test_array_typearg";

    let mut program = compile_source(STUB_SOURCE);
    let fn_obj_idx = inject_function(
        &mut program,
        fn_name,
        vec![Instruction::LoadType(0), Instruction::Return],
        vec![ConstValue::Type(template)],
    );

    let function_index = program.function_index(fn_name).expect("function not found");
    assert_eq!(function_index, fn_obj_idx);

    let mut vm =
        BexVm::from_program(program, Arc::new(AtomicBool::new(false))).expect("from_program");
    let function_ptr = vm.heap.compile_time_ptr(function_index);
    vm.set_entry_point(function_ptr, &[]);

    // Inject type_args
    {
        use bex_vm::Frame;
        let frame = vm.frames.last_mut().expect("entry frame must exist");
        if let Frame::Bytecode(bf) = frame {
            bf.type_args = vec![RealizedTy::int()];
        } else {
            panic!("entry frame should be Bytecode");
        }
    }

    let result = loop {
        match vm.exec().expect("exec") {
            VmExecState::Complete(v) => break v,
            VmExecState::EarlyYield => continue,
            other => panic!("unexpected state: {other:?}"),
        }
    };

    let Some(ptr) = result.as_object_ptr() else {
        panic!("expected Object, got {result:?}");
    };
    let obj = vm.get_object(ptr);
    match obj {
        Object::Type(type_value) => {
            assert_eq!(
                type_value.ty,
                RealizedTy::list(RealizedTy::int()),
                "Array(TypeArgRef(0)) with int → int[]"
            );
        }
        other => panic!("expected Object::Type, got {other:?}"),
    }
}

// ─── 3.3 & 3.5 ── Call { ntypeargs } pops type args from stack ──────────────

/// Verify that `Call { callee, ntypeargs: 1 }` pops one `Object::Type` value
/// from below the regular args and injects it into the callee's `type_args`.
///
/// Setup:
///   - outer function: pushes `Object::Type(string)` via `LoadType`, then
///     `Call { callee=inner_fn, ntypeargs=1 }`
///   - inner function: immediately runs `LoadType(TypeArgRef(0))` and returns
///
/// The inner function should see `RuntimeTy::string()` in `frame.type_args[0]` and
/// return `Object::Type(string)`.
#[test]
fn call_ntypeargs_threads_type_arg_into_callee() {
    let fn_outer = "user.call_outer";
    let fn_inner = "user.call_inner";

    let mut program = compile_source(STUB_SOURCE);

    // Register inner function first so we know its global slot.
    let inner_obj_idx = inject_function(
        &mut program,
        fn_inner,
        vec![
            // constants[0] = TypeArgRef(0) — resolved from frame.type_args[0]
            Instruction::LoadType(0),
            Instruction::Return,
        ],
        vec![ConstValue::Type(TyTemplate::TypeArgRef(0))],
    );
    let inner_global_slot = program
        .function_global_indices
        .get(fn_inner)
        .copied()
        .expect("inner global slot");

    // Outer function: push type arg (string), call inner with ntypeargs=1
    inject_function(
        &mut program,
        fn_outer,
        vec![
            Instruction::LoadType(0), // push Object::Type(string) as type arg
            Instruction::Call {
                callee: GlobalIndex::from_raw(inner_global_slot),
                ntypeargs: 1,
            },
            Instruction::Return,
        ],
        vec![ConstValue::Type(TyTemplate::from(
            baml_type::RealizedTy::string(),
        ))],
    );
    let _ = inner_obj_idx; // suppress unused warning

    let outer_idx = program.function_index(fn_outer).expect("outer not found");

    let mut vm =
        BexVm::from_program(program, Arc::new(AtomicBool::new(false))).expect("from_program");
    let function_ptr = vm.heap.compile_time_ptr(outer_idx);
    vm.set_entry_point(function_ptr, &[]);

    let result = loop {
        match vm.exec().expect("exec") {
            VmExecState::Complete(v) => break v,
            VmExecState::EarlyYield => continue,
            other => panic!("unexpected state: {other:?}"),
        }
    };

    let Some(ptr) = result.as_object_ptr() else {
        panic!("expected Object, got {result:?}");
    };
    let obj = vm.get_object(ptr);
    match obj {
        Object::Type(type_value) => {
            assert_eq!(
                type_value.ty,
                RealizedTy::string(),
                "inner function should receive RealizedTy::string() via type arg"
            );
        }
        other => panic!("expected Object::Type, got {other:?}"),
    }
}
