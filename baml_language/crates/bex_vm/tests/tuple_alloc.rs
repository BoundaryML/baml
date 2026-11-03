//! Bytecode-level tests for tuple allocation and element reads.
//!
//! BAML source cannot reach these directly: it has no spelling for the
//! zero-arity tuple, the checker rejects a tuple index outside the tuple and a
//! store to a slot, and the compiler always loads a tuple type of the
//! allocation's own arity. These tests hand the VM that bytecode to pin the
//! runtime's own contract: `AllocTuple` builds the tuple at the loaded type
//! (frame references substituted), `AllocTuple(0)` is valid, a type of another
//! arity is refused, and `LoadArrayElement` reads a slot with its own bounds
//! check while `StoreArrayElement` refuses a tuple.

use std::sync::{Arc, atomic::AtomicBool};

use baml_test_support::compile_source;
use bex_vm::{BexVm, Frame, VmExecState};
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
    let func = Function {
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
    };
    program.add_object(Object::Function(Box::new(func)))
}

/// Run `instructions` as an entry point whose frame holds `type_args`.
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
    // The frame's `int?`, not the `int` the element holds.
    assert_eq!(
        &*tuple.element_tys,
        &[
            RealizedTy::optional(RealizedTy::int()),
            RealizedTy::string()
        ]
    );
    assert_eq!(&*tuple.data, &[Value::int(1), Value::int(2)]);
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

/// `t[index]` over the two-slot tuple `(10, 20)`.
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
fn load_array_element_reads_a_tuple_slot() {
    assert_eq!(load_slot(0), Ok(Value::int(10)));
    assert_eq!(load_slot(1), Ok(Value::int(20)));
}

#[test]
fn load_array_element_bounds_checks_a_tuple_index() {
    // Past the end, and negative: a tuple index never counts from the end.
    // Either throws the ordinary index-out-of-range panic.
    for index in [2, -1] {
        let err = load_slot(index).expect_err("out of range");
        assert!(err.contains("ThrownUnhandled"), "{err}");
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
    let err = result.expect_err("a tuple slot is immutable");
    assert!(err.contains("TypeError"), "{err}");
}
