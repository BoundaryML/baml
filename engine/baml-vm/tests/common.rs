//! Common test utilities for VM tests.
//!
//! Re-exports types from baml_vm::test and adds helper functions that require baml-compiler.

// Re-export all types from baml_vm::test
// Additional imports for helper functions
use baml_compiler::test::ast;
pub use baml_vm::test::*;
use baml_vm::{
    watch::Watch, BamlVmProgram, Bytecode, EvalStack, Frame, Function, FunctionKind, GlobalPool,
    Instruction as VmInstruction, Object as VmObject, ObjectIndex, ObjectPool, StackIndex,
    Value as VmValue, Vm, VmExecState,
};

/// Helper struct for testing VM execution.
pub struct ProgramInput<Expect> {
    pub source: &'static str,
    pub function: &'static str,
    pub expected: Expect,
}

pub type Program = ProgramInput<ExecState>;
pub type FailingProgram = ProgramInput<baml_vm::errors::VmError>;

pub fn assert_vm_fails(input: FailingProgram) -> anyhow::Result<()> {
    assert_vm_fails_with_inspection(input, |_vm| Ok(()))
}

pub fn assert_vm_fails_with_inspection(
    input: FailingProgram,
    inspect: impl FnOnce(&Vm) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let (mut vm, mut result) = setup_and_exec_program(input.source, input.function)?;

    loop {
        match result {
            Err(err) => {
                assert_eq!(
                    err, input.expected,
                    "VM execution result mismatch for function '{}'",
                    input.function
                );
                inspect(&vm)?;
                return Ok(());
            }
            Ok(state) => {
                // Keep stepping until we hit the expected error.
                if matches!(state, VmExecState::Complete(_)) {
                    panic!(
                        "VM unexpectedly completed without error for function '{}'",
                        input.function
                    );
                }
                result = vm.exec();
            }
        }
    }
}

#[track_caller]
pub fn assert_vm_executes(input: Program) -> anyhow::Result<()> {
    assert_vm_executes_with_inspection(input, |_vm| Ok(()))
}

#[track_caller]
pub fn assert_vm_executes_with_inspection(
    input: Program,
    inspect: impl FnOnce(&Vm) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let (vm, states) = collect_vm_exec_states(input.source, input.function)?;
    let test_result = states
        .last()
        .cloned()
        .expect("execution should produce at least one state");

    assert_eq!(
        test_result, input.expected,
        "VM execution result mismatch for function '{}'",
        input.function
    );

    inspect(&vm)?;

    Ok(())
}

/// Collects all VM execution states by repeatedly calling exec() until completion.
pub fn collect_vm_exec_states(
    source: &'static str,
    function: &str,
) -> anyhow::Result<(Vm, Vec<ExecState>)> {
    let ast = ast(source)?;
    let BamlVmProgram {
        objects,
        globals,
        resolved_function_names,
        resolved_enums_names: _,
        resolved_class_names: _,
    } = baml_compiler::compile(&ast)?;
    let (target_function_index, _) = resolved_function_names[function];
    let mut vm = Vm {
        frames: vec![Frame {
            function: target_function_index,
            instruction_ptr: 0,
            locals_offset: StackIndex::from_raw(0),
        }],
        stack: EvalStack::from_vec(vec![VmValue::Object(target_function_index)]),
        runtime_allocs_offset: ObjectIndex::from_raw(objects.len()),
        objects,
        globals,
        env_vars: Default::default(),
        watch: Watch::new(),
        watched_vars: Default::default(),
        interrupt_frame: None,
    };

    let mut states = Vec::new();

    loop {
        let result = vm.exec()?;
        let is_complete = matches!(result, VmExecState::Complete(_));
        let test_state = ExecState::from_vm_exec_state(result, &vm)?;
        states.push(test_state);

        if is_complete {
            break;
        }
    }

    Ok((vm, states))
}

/// Helper type for testing VM execution with expected Emit states.
pub type WatchProgram = ProgramInput<Vec<Vec<Notification>>>;

#[track_caller]
pub fn assert_vm_emits(input: WatchProgram) -> anyhow::Result<()> {
    assert_vm_emits_with_inspection(input, |_vm, _states| Ok(()))
}

#[track_caller]
pub fn assert_vm_emits_with_inspection(
    input: WatchProgram,
    inspect: impl FnOnce(&Vm, &[ExecState]) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let (vm, states) = collect_vm_exec_states(input.source, input.function)?;

    let emit_states: Vec<Vec<Notification>> = states
        .iter()
        .filter_map(|state| match state {
            ExecState::Emit(roots) => {
                let filtered: Vec<Notification> = roots
                    .iter()
                    .cloned()
                    .filter(|n| !matches!(n, Notification::Viz { .. }))
                    .collect();
                if filtered.is_empty() {
                    None
                } else {
                    Some(filtered)
                }
            }
            _ => None,
        })
        .collect();

    assert_eq!(
        emit_states, input.expected,
        "VM emit states mismatch for function '{}'",
        input.function
    );

    inspect(&vm, &states)?;

    Ok(())
}

fn setup_and_exec_program(
    source: &'static str,
    function: &str,
) -> Result<(Vm, Result<VmExecState, baml_vm::errors::VmError>), anyhow::Error> {
    let ast = ast(source)?;
    let BamlVmProgram {
        objects,
        globals,
        resolved_function_names,
        resolved_enums_names: _,
        resolved_class_names: _,
    } = baml_compiler::compile(&ast)?;
    let (target_function_index, _) = resolved_function_names[function];
    let mut vm = Vm {
        frames: vec![Frame {
            function: target_function_index,
            instruction_ptr: 0,
            locals_offset: StackIndex::from_raw(0),
        }],
        stack: EvalStack::from_vec(vec![VmValue::Object(target_function_index)]),
        runtime_allocs_offset: ObjectIndex::from_raw(objects.len()),
        objects,
        globals,
        env_vars: Default::default(),
        watch: Watch::new(),
        watched_vars: Default::default(),
        interrupt_frame: None,
    };
    let result = vm.exec();
    Ok((vm, result))
}

/// Helper struct for testing VM execution with direct bytecode.
pub struct BytecodeProgram {
    pub arity: usize,
    pub instructions: Vec<VmInstruction>,
    pub constants: Vec<VmValue>,
    pub expected: VmExecState,
}

pub fn assert_vm_executes_bytecode(input: BytecodeProgram) -> anyhow::Result<()> {
    assert_vm_executes_bytecode_with_inspection(input, |_vm, _result| Ok(()))
}

pub fn assert_vm_executes_bytecode_with_inspection(
    input: BytecodeProgram,
    inspect: impl FnOnce(&Vm, VmExecState) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
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
        span: internal_baml_diagnostics::Span::fake(),
        viz_nodes: Vec::new(),
    };

    let objects = vec![VmObject::Function(function)];
    let globals = vec![VmValue::Object(ObjectIndex::from_raw(0))];

    let mut vm = Vm {
        frames: vec![Frame {
            function: ObjectIndex::from_raw(0),
            instruction_ptr: 0,
            locals_offset: StackIndex::from_raw(0),
        }],
        stack: EvalStack::from_vec(vec![VmValue::Object(ObjectIndex::from_raw(0))]),
        runtime_allocs_offset: ObjectIndex::from_raw(objects.len()),
        objects: ObjectPool::from_vec(objects),
        globals: GlobalPool::from_vec(globals),
        env_vars: Default::default(),
        watch: Watch::new(),
        watched_vars: Default::default(),
        interrupt_frame: None,
    };

    let result = vm.exec()?;

    assert_eq!(
        result, input.expected,
        "VM execution result mismatch for bytecode test",
    );

    inspect(&vm, result)?;

    Ok(())
}
