//! Common test utilities for VM tests.
//!
//! Re-exports types from `baml_vm::test` and adds helper functions that use the new compiler.

#![allow(dead_code)] // Test utilities may not all be used yet
#![allow(clippy::needless_pass_by_value)] // Test utilities intentionally take ownership

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, atomic::AtomicU32},
};

use baml_base::{FileId, SourceFile};
pub(crate) use baml_vm::test::*;
use baml_vm::{
    EvalStack, Frame, GlobalPool, ObjectIndex, ObjectPool, StackIndex, Value as VmValue, Vm,
    VmExecState, watch::Watch,
};

//
// ──────────────────────────────────────────────────────── TEST DATABASE ─────
//

/// Minimal test database for compilation tests.
///
/// This is a stripped-down version of `baml_db::RootDatabase` that implements
/// just enough to run `compile_files`. This avoids a dependency cycle between
/// `baml_codegen` and `baml_db`.
#[salsa::db]
#[derive(Clone)]
struct TestDatabase {
    storage: salsa::Storage<Self>,
    next_file_id: Arc<AtomicU32>,
}

#[salsa::db]
impl salsa::Database for TestDatabase {}

#[salsa::db]
impl baml_hir::Db for TestDatabase {}

#[salsa::db]
impl baml_thir::Db for TestDatabase {}

impl TestDatabase {
    fn new() -> Self {
        Self {
            storage: salsa::Storage::default(),
            next_file_id: Arc::new(AtomicU32::new(0)),
        }
    }

    fn add_file(&mut self, path: impl Into<PathBuf>, text: impl Into<String>) -> SourceFile {
        let file_id = FileId::new(
            self.next_file_id
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst),
        );
        SourceFile::new(self, text.into(), path.into(), file_id)
    }
}

/// Helper struct for testing VM execution.
pub(crate) struct ProgramInput<Expect> {
    pub source: &'static str,
    pub function: &'static str,
    pub expected: Expect,
}

pub(crate) type Program = ProgramInput<ExecState>;
pub(crate) type FailingProgram = ProgramInput<baml_vm::errors::VmError>;

/// Compile BAML source and return a VM program.
fn compile_source(source: &str) -> baml_codegen::Program {
    let mut db = TestDatabase::new();
    let file = db.add_file("test.baml", source);
    baml_codegen::compile_files(&db, &[file])
}

pub(crate) fn assert_vm_fails(input: FailingProgram) -> anyhow::Result<()> {
    assert_vm_fails_with_inspection(input, |_vm| Ok(()))
}

pub(crate) fn assert_vm_fails_with_inspection(
    input: FailingProgram,
    inspect: impl FnOnce(&Vm) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let (vm, result) = setup_and_exec_program(input.source, input.function)?;

    assert_eq!(
        result,
        Err(input.expected),
        "VM execution result mismatch for function '{}'",
        input.function
    );

    inspect(&vm)?;

    Ok(())
}

#[track_caller]
pub(crate) fn assert_vm_executes(input: Program) -> anyhow::Result<()> {
    assert_vm_executes_with_inspection(input, |_vm| Ok(()))
}

#[track_caller]
pub(crate) fn assert_vm_executes_with_inspection(
    input: Program,
    inspect: impl FnOnce(&Vm) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let (vm, result) = setup_and_exec_program(input.source, input.function)?;
    let result = result?;

    let test_result = ExecState::from_vm_exec_state(result, &vm)?;

    assert_eq!(
        test_result, input.expected,
        "VM execution result mismatch for function '{}'",
        input.function
    );

    inspect(&vm)?;

    Ok(())
}

/// Collects all VM execution states by repeatedly calling `exec()` until completion.
pub(crate) fn collect_vm_exec_states(
    source: &'static str,
    function: &str,
) -> anyhow::Result<(Vm, Vec<ExecState>)> {
    let program = compile_source(source);

    let target_function_index = *program
        .function_indices
        .get(function)
        .ok_or_else(|| anyhow::anyhow!("function '{function}' not found"))?;

    let mut vm = Vm {
        frames: vec![Frame {
            function: ObjectIndex::from_raw(target_function_index),
            instruction_ptr: 0,
            locals_offset: StackIndex::from_raw(0),
        }],
        stack: EvalStack::from_vec(vec![VmValue::Object(ObjectIndex::from_raw(
            target_function_index,
        ))]),
        runtime_allocs_offset: ObjectIndex::from_raw(program.objects.len()),
        objects: program.objects.clone(),
        globals: program.globals.clone(),
        env_vars: HashMap::default(),
        watch: Watch::new(),
        watched_vars: HashMap::default(),
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
pub(crate) type WatchProgram = ProgramInput<Vec<Vec<Notification>>>;

#[track_caller]
pub(crate) fn assert_vm_emits(input: WatchProgram) -> anyhow::Result<()> {
    assert_vm_emits_with_inspection(input, |_vm, _states| Ok(()))
}

#[track_caller]
pub(crate) fn assert_vm_emits_with_inspection(
    input: WatchProgram,
    inspect: impl FnOnce(&Vm, &[ExecState]) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let (vm, states) = collect_vm_exec_states(input.source, input.function)?;

    let emit_states: Vec<Vec<Notification>> = states
        .iter()
        .filter_map(|state| match state {
            ExecState::Emit(roots) => Some(roots.clone()),
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
    let program = compile_source(source);

    let target_function_index = *program
        .function_indices
        .get(function)
        .ok_or_else(|| anyhow::anyhow!("function '{function}' not found"))?;

    let mut vm = Vm {
        frames: vec![Frame {
            function: ObjectIndex::from_raw(target_function_index),
            instruction_ptr: 0,
            locals_offset: StackIndex::from_raw(0),
        }],
        stack: EvalStack::from_vec(vec![VmValue::Object(ObjectIndex::from_raw(
            target_function_index,
        ))]),
        runtime_allocs_offset: ObjectIndex::from_raw(program.objects.len()),
        objects: program.objects.clone(),
        globals: program.globals.clone(),
        env_vars: HashMap::default(),
        watch: Watch::new(),
        watched_vars: HashMap::default(),
        interrupt_frame: None,
    };
    let result = vm.exec();
    Ok((vm, result))
}

/// Helper struct for testing VM execution with direct bytecode.
pub(crate) struct BytecodeProgram {
    pub arity: usize,
    pub instructions: Vec<baml_vm::Instruction>,
    pub constants: Vec<VmValue>,
    pub expected: VmExecState,
}

pub(crate) fn assert_vm_executes_bytecode(input: BytecodeProgram) -> anyhow::Result<()> {
    assert_vm_executes_bytecode_with_inspection(input, |_vm, _result| Ok(()))
}

pub(crate) fn assert_vm_executes_bytecode_with_inspection(
    input: BytecodeProgram,
    inspect: impl FnOnce(&Vm, VmExecState) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let function = baml_vm::Function {
        name: "test_fn".to_string(),
        arity: input.arity,
        bytecode: baml_vm::Bytecode {
            source_lines: vec![1; input.instructions.len()],
            scopes: vec![0; input.instructions.len()],
            instructions: input.instructions,
            constants: input.constants,
        },
        kind: baml_vm::FunctionKind::Exec,
        locals_in_scope: {
            let mut names = Vec::with_capacity(input.arity + 1);
            names.push("<fn test_fn>".to_string());
            names.resize_with(names.capacity(), String::new);
            vec![names]
        },
        span: baml_base::Span::fake(),
        block_notifications: Vec::new(),
    };

    let objects = vec![baml_vm::Object::Function(function)];
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
        env_vars: HashMap::default(),
        watch: Watch::new(),
        watched_vars: HashMap::default(),
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
