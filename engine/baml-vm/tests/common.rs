//! Common test utilities for VM tests.

use baml_compiler::test::ast;
use baml_vm::{
    BamlVmProgram, Bytecode, EvalStack, Frame, Function, FunctionKind, GlobalPool, Instruction,
    Object, ObjectIndex, ObjectPool, StackIndex, Value, Vm, VmError, VmExecState,
};

/// Helper struct for testing VM execution.
pub struct ProgramInput<Expect> {
    pub source: &'static str,
    pub function: &'static str,
    pub expected: Expect,
}

pub type Program = ProgramInput<VmExecState>;
pub type FailingProgram = ProgramInput<VmError>;

/// Unified helper function for VM execution with optional inspection.
pub fn assert_vm_executes(input: Program) -> anyhow::Result<()> {
    assert_vm_executes_with_inspection(input, |_vm| Ok(()))
}

/// Helper function for VM execution with custom inspection.
pub fn assert_vm_executes_with_inspection(
    input: Program,
    inspect: impl FnOnce(&Vm) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let (vm, result) = setup_and_exec_program(input.source, input.function)?;
    let result = result?;

    assert_eq!(
        result, input.expected,
        "VM execution result mismatch for function '{}'",
        input.function
    );

    // Run custom inspection
    inspect(&vm)?;

    Ok(())
}

pub fn assert_vm_fails(input: FailingProgram) -> anyhow::Result<()> {
    let (_, result) = setup_and_exec_program(input.source, input.function)?;

    assert_eq!(
        result,
        Err(input.expected),
        "VM execution result mismatch for function '{}'",
        input.function
    );

    Ok(())
}

fn setup_and_exec_program(
    source: &'static str,
    function: &str,
) -> Result<(Vm, Result<VmExecState, VmError>), anyhow::Error> {
    let ast = ast(source)?;
    let BamlVmProgram {
        objects,
        globals,
        resolved_function_names,
        resolved_enums_names,
        resolved_class_names,
    } = baml_compiler::compile(&ast)?;
    let (target_function_index, _) = resolved_function_names[function];
    let mut vm = Vm {
        frames: vec![Frame {
            function: target_function_index,
            instruction_ptr: 0,
            locals_offset: StackIndex::from_raw(0),
        }],
        stack: EvalStack::from_vec(vec![Value::Object(target_function_index)]),
        runtime_allocs_offset: ObjectIndex::from_raw(objects.len()),
        objects,
        globals,
    };
    let result = vm.exec();
    Ok((vm, result))
}

/// Helper struct for testing VM execution with direct bytecode.
pub struct BytecodeProgram {
    pub arity: usize,
    pub instructions: Vec<Instruction>,
    pub constants: Vec<Value>,
    pub expected: VmExecState,
}

/// Helper function for VM execution with direct bytecode.
pub fn assert_vm_executes_bytecode(input: BytecodeProgram) -> anyhow::Result<()> {
    assert_vm_executes_bytecode_with_inspection(input, |_vm, _result| Ok(()))
}

/// Helper function for VM execution with direct bytecode and custom inspection.
pub fn assert_vm_executes_bytecode_with_inspection(
    input: BytecodeProgram,
    inspect: impl FnOnce(&Vm, VmExecState) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    // Create function from bytecode
    let function = Function {
        name: "test_fn".to_string(),
        arity: input.arity,
        bytecode: Bytecode {
            source_lines: vec![1; input.instructions.len()],
            scopes: vec![0; input.instructions.len()],
            instructions: input.instructions,
            constants: input.constants,
        },
        kind: FunctionKind::Exec,
        locals_in_scope: {
            let mut names = Vec::with_capacity(input.arity + 1);
            names.push("<fn test_fn>".to_string());
            names.resize_with(names.capacity(), String::new);
            vec![names]
        },
    };

    let objects = vec![Object::Function(function)];
    let globals = vec![Value::Object(ObjectIndex::from_raw(0))];

    // Create and run the VM
    let mut vm = Vm {
        frames: vec![Frame {
            function: ObjectIndex::from_raw(0),
            instruction_ptr: 0,
            locals_offset: StackIndex::from_raw(0),
        }],
        stack: EvalStack::from_vec(vec![Value::Object(ObjectIndex::from_raw(0))]),
        runtime_allocs_offset: ObjectIndex::from_raw(objects.len()),
        objects: ObjectPool::from_vec(objects),
        globals: GlobalPool::from_vec(globals),
    };

    let result = vm.exec()?;

    assert_eq!(
        result, input.expected,
        "VM execution result mismatch for bytecode test",
    );

    // Run custom inspection
    inspect(&vm, result)?;

    Ok(())
}
