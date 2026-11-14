use std::sync::{Arc, atomic::AtomicBool};

use baml_test_support::compile_source;
use bex_vm::{
    BexVm, Frame, VmExecState,
    package_baml::{BamlPackageBaml, NativeCallResult, PackageBamlImpl},
};
use bex_vm_types::{
    ConstValue, Instruction, Object, RealizedTy, TyTemplate, Value,
    bytecode::Bytecode,
    types::{Function, FunctionKind, FunctionOrigin, Program},
};

const STUB_SOURCE: &str = r#"
function _stub() -> int {
    42
}
"#;

fn inject_function(
    program: &mut Program,
    fn_name: &str,
    instructions: Vec<Instruction>,
    constants: Vec<ConstValue>,
) -> usize {
    let func = make_function(fn_name, instructions, constants);
    program.add_object(Object::Function(Box::new(func)))
}

fn make_function(
    fn_name: &str,
    instructions: Vec<Instruction>,
    constants: Vec<ConstValue>,
) -> Function {
    Function {
        identity: bex_vm_types::types::FunctionIdentity::Declared(
            bex_vm_types::types::FnPath::Free(bex_vm_types::types::LocalName::new(
                Vec::new(),
                baml_type::Name::new(fn_name),
            )),
        ),
        source_file: String::new(),
        docstring: None,
        arity: 0,
        real_local_count: 0,
        bytecode: Bytecode {
            instructions,
            constants,
            ..Bytecode::default()
        },
        kind: FunctionKind::Bytecode,
        telemetry_function_id: None,
        telemetry_registration: bex_vm_types::FunctionRegistration::default(),
        telemetry_policy_id: bex_vm_types::TelemetryPolicyId::none(),
        compiled_body: None,
        local_names: vec![],
        debug_locals: vec![],
        span: baml_type::Span::fake(),
        return_type: baml_type::TyTemplate::Unknown,
        param_names: vec![],
        param_types: vec![],
        param_has_default: vec![],
        display_type_params: vec![],
        type_param_names: Vec::new(),
        generic_param_bounds: vec![],
        display_param_types: vec![],
        display_return_type: "unknown".to_string(),
        throws_type: baml_type::TyTemplate::Never,
        origin: FunctionOrigin::UserDefined,
        native_key: None,
        body_meta: None,
        trace_passthrough: false,
        owner: bex_vm_types::HeapPtr::null(),
    }
}

fn run(
    instructions: Vec<Instruction>,
    constants: Vec<ConstValue>,
    type_args: Vec<RealizedTy>,
) -> (Result<Value, String>, BexVm) {
    let mut program = compile_source(STUB_SOURCE);
    let function_index = inject_function(&mut program, "user.tuple_test", instructions, constants);
    let mut vm =
        BexVm::from_program(program, Arc::new(AtomicBool::new(false))).expect("from_program");
    let function_ptr = vm.heap.compile_time_ptr(function_index);
    vm.set_entry_point(function_ptr, &[]);
    if let Some(Frame::Bytecode(frame)) = vm.frames.last_mut() {
        frame.type_args = type_args;
    } else {
        panic!("entry frame should be Bytecode");
    }
    let result = loop {
        match vm.exec() {
            Ok(VmExecState::Complete(value)) => break Ok(value),
            Ok(VmExecState::EarlyYield) => continue,
            Ok(other) => panic!("unexpected VM state: {other:?}"),
            Err(err) => break Err(format!("{err:?}")),
        }
    };
    (result, vm)
}

fn int() -> TyTemplate {
    TyTemplate::from(baml_type::RealizedTy::int())
}

fn tuple_of(vm: &BexVm, value: Value) -> bex_vm_types::Tuple {
    let ptr = value.as_object_ptr().expect("a tuple is an object");
    match vm.get_object(ptr) {
        Object::Tuple(tuple) => tuple.clone(),
        other => panic!("expected a tuple, got {other:?}"),
    }
}

#[test]
fn alloc_tuple_carries_the_loaded_type_with_frame_arguments() {
    let template = TyTemplate::Tuple(Box::new([
        TyTemplate::TypeArgRef(0),
        TyTemplate::from(baml_type::RealizedTy::string()),
    ]));
    let (result, vm) = run(
        vec![
            Instruction::LoadConst(0),
            Instruction::LoadConst(1),
            Instruction::LoadType(2),
            Instruction::AllocTuple(2),
            Instruction::Return,
        ],
        vec![
            ConstValue::Int(1),
            ConstValue::Int(2),
            ConstValue::Type(template),
        ],
        vec![RealizedTy::optional(RealizedTy::int())],
    );
    let tuple = tuple_of(&vm, result.expect("allocates"));
    assert_eq!(
        &*tuple.element_tys,
        &[
            RealizedTy::optional(RealizedTy::int()),
            RealizedTy::string()
        ]
    );
    assert_eq!(
        tuple.slot_values().collect::<Vec<_>>(),
        [Value::int(1), Value::int(2)]
    );
}

#[test]
fn alloc_tuple_of_zero_arity_is_valid() {
    let (result, vm) = run(
        vec![
            Instruction::LoadType(0),
            Instruction::AllocTuple(0),
            Instruction::Return,
        ],
        vec![ConstValue::Type(TyTemplate::Tuple(Box::new([])))],
        vec![],
    );
    let tuple = tuple_of(&vm, result.expect("allocates"));
    assert!(tuple.element_tys.is_empty());
    assert!(tuple.is_empty());
}

#[test]
fn alloc_tuple_refuses_a_type_of_another_arity() {
    let (result, _) = run(
        vec![
            Instruction::LoadConst(0),
            Instruction::LoadConst(0),
            Instruction::LoadType(1),
            Instruction::AllocTuple(2),
            Instruction::Return,
        ],
        vec![
            ConstValue::Int(1),
            ConstValue::Type(TyTemplate::Tuple(Box::new([int()]))),
        ],
        vec![],
    );
    let err = result.expect_err("the arity disagrees");
    assert!(err.contains("TupleTypeMismatch"), "{err}");
}

fn load_slot(index: i64) -> Result<Value, String> {
    let pair = TyTemplate::Tuple(Box::new([int(), int()]));
    run(
        vec![
            Instruction::LoadConst(0),
            Instruction::LoadConst(1),
            Instruction::LoadType(2),
            Instruction::AllocTuple(2),
            Instruction::LoadConst(3),
            Instruction::LoadArrayElement,
            Instruction::Return,
        ],
        vec![
            ConstValue::Int(10),
            ConstValue::Int(20),
            ConstValue::Type(pair),
            ConstValue::Int(index),
        ],
        vec![],
    )
    .0
}

#[test]
fn load_array_element_refuses_a_tuple() {
    for index in [0, 1, 2, -1] {
        let err = load_slot(index).expect_err("a tuple is not an array");
        assert!(err.contains("TypeError"), "{err}");
    }
}

#[test]
fn store_array_element_refuses_a_tuple() {
    let pair = TyTemplate::Tuple(Box::new([int()]));
    let (result, _) = run(
        vec![
            Instruction::LoadConst(0),
            Instruction::LoadType(1),
            Instruction::AllocTuple(1),
            Instruction::LoadConst(2),
            Instruction::LoadConst(0),
            Instruction::StoreArrayElement,
            Instruction::LoadConst(0),
            Instruction::Return,
        ],
        vec![
            ConstValue::Int(1),
            ConstValue::Type(pair),
            ConstValue::Int(0),
        ],
        vec![],
    );
    let err = result.expect_err("a tuple is not an array");
    assert!(err.contains("TypeError"), "{err}");
}

fn alloc_pair() -> (Vec<Instruction>, Vec<ConstValue>) {
    (
        vec![
            Instruction::LoadConst(0),
            Instruction::LoadConst(1),
            Instruction::LoadType(2),
            Instruction::AllocTuple(2),
        ],
        vec![
            ConstValue::Int(10),
            ConstValue::Int(20),
            ConstValue::Type(TyTemplate::Tuple(Box::new([int(), int()]))),
        ],
    )
}

fn load_tuple_field(slot: usize) -> Result<Value, String> {
    let (mut instructions, constants) = alloc_pair();
    instructions.extend([Instruction::LoadTupleField(slot), Instruction::Return]);
    run(instructions, constants, vec![]).0
}

#[test]
fn load_tuple_field_reads_a_slot() {
    assert_eq!(load_tuple_field(0), Ok(Value::int(10)));
    assert_eq!(load_tuple_field(1), Ok(Value::int(20)));
}

#[test]
fn load_tuple_field_bounds_checks_the_slot() {
    let err = load_tuple_field(2).expect_err("out of range");
    assert!(err.contains("ThrownUnhandled"), "{err}");
}

#[test]
fn tuple_and_class_field_instructions_refuse_each_others_receivers() {
    let err = run(
        vec![
            Instruction::LoadType(0),
            Instruction::AllocArray(0),
            Instruction::LoadTupleField(0),
            Instruction::Return,
        ],
        vec![ConstValue::Type(int())],
        vec![],
    )
    .0
    .expect_err("a list is not a tuple");
    assert!(err.contains("TypeError"), "{err}");
    for access in [
        vec![Instruction::LoadField(0)],
        vec![
            Instruction::LoadConst(0),
            Instruction::StoreField(0),
            Instruction::LoadConst(0),
        ],
    ] {
        let (mut instructions, constants) = alloc_pair();
        instructions.extend(access);
        instructions.push(Instruction::Return);
        let err = run(instructions, constants, vec![])
            .0
            .expect_err("a tuple is not an instance");
        assert!(err.contains("TypeError"), "{err}");
    }
}

fn tuple_with_child_cycle(second: i64) -> (Value, BexVm) {
    let (_, mut vm) = run(
        vec![Instruction::LoadConst(0), Instruction::Return],
        vec![ConstValue::Null],
        vec![],
    );
    let value = alloc_tuple_with_child_cycle(&mut vm, second);
    (value, vm)
}

fn alloc_tuple_with_child_cycle(vm: &mut BexVm, second: i64) -> Value {
    let child = vm.tlab.alloc_array(RealizedTy::Unknown, vec![]);
    let tuple = bex_vm_types::Tuple::new(
        [
            RealizedTy::List(Box::new(RealizedTy::Unknown)),
            RealizedTy::int(),
        ],
        [Value::object(child), Value::int(second)],
    )
    .unwrap();
    let value = Value::object(vm.tlab.alloc_tuple(tuple));
    let Object::Array(array) = vm.get_object(child) else {
        unreachable!()
    };
    array.lock_mut(vm.tlab.alloc_debt()).push(value);
    value
}

fn done(result: NativeCallResult) -> Value {
    match result {
        NativeCallResult::Done(value) => value,
        NativeCallResult::Error(e) => panic!("errored: {e:?}"),
        NativeCallResult::YieldToCall { .. } => panic!("unexpectedly yielded"),
    }
}

#[test]
fn equality_terminates_on_a_cycle_through_a_tuple_child() {
    let (one, mut vm) = tuple_with_child_cycle(1);
    let two = alloc_tuple_with_child_cycle(&mut vm, 2);
    let other_one = alloc_tuple_with_child_cycle(&mut vm, 1);
    for (a, b, expected) in [(one, other_one, true), (one, two, false), (one, one, true)] {
        let equal = done(PackageBamlImpl::_equals_structural_default(&mut vm, &a, &b));
        assert_eq!(equal.as_bool(), Some(expected));
    }
}

#[test]
fn rendering_terminates_on_a_cycle_through_a_tuple_child() {
    let (one, mut vm) = tuple_with_child_cycle(1);
    let rendered = done(PackageBamlImpl::_to_string_default(&mut vm, &one));
    let Object::String(rendered) = vm.get_object(rendered.as_object_ptr().unwrap()) else {
        panic!("renders to a string")
    };
    assert_eq!(rendered.as_str(), "([…], 1)");
}

#[test]
fn deep_copy_preserves_the_cycle_through_a_tuple_child() {
    let (one, mut vm) = tuple_with_child_cycle(1);
    let copy = PackageBamlImpl::deep_copy(&mut vm, &one).expect("copies");
    assert_ne!(copy, one);
    let tuple = tuple_of(&vm, copy);
    let Object::Array(child) = vm.get_object(tuple.load_slot(0).as_object_ptr().unwrap()) else {
        panic!("a child list")
    };
    assert_eq!(child.lock()[0], copy);
    assert_eq!(tuple.load_slot(1), Value::int(1));
    assert_eq!(
        &*tuple.element_tys,
        &[
            RealizedTy::List(Box::new(RealizedTy::Unknown)),
            RealizedTy::int()
        ]
    );
}

#[test]
fn tuple_field_instructions_disassemble_with_their_slot() {
    let (mut instructions, constants) = alloc_pair();
    instructions.extend([
        Instruction::Copy(0),
        Instruction::LoadTupleField(1),
        Instruction::Pop(1),
        Instruction::Return,
    ]);
    let function = make_function("user.disasm", instructions, constants);
    let textual = bex_vm::debug::display_program(
        &[("disasm".to_string(), &function)],
        bex_vm::debug::BytecodeFormat::Textual,
    );
    assert!(textual.contains("load_tuple_field .1"), "{textual}");

    let compact = function.bytecode.lower_to_compact();
    let mut raw = String::new();
    bex_vm::debug::display_compact_bytecode(&compact, &function.bytecode.constants, &mut raw)
        .expect("decodes");
    let rows: Vec<String> = raw
        .lines()
        .map(|line| line.split_whitespace().collect())
        .collect();
    assert!(rows.contains(&"0019LOAD_TUPLE_FIELD1".to_string()), "{raw}");
    assert_eq!(rows.last().map(String::as_str), Some("0029RETURN"), "{raw}");
}
