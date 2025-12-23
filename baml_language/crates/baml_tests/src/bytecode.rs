//! Shared test utilities for BAML bytecode testing.
//!
//! This module provides common infrastructure for testing bytecode compilation
//! and execution in `baml_vm`.
//!
//! # Contents
//!
//! - [`TestDatabase`]: A minimal salsa database for compilation.
//! - [`compile_source`]: Compiles BAML source to a VM program.
//! - [`assert_vm_executes`], [`assert_vm_fails`]: Test assertion helpers.
//! - [`Program`], [`FailingProgram`]: Test input types.
//!
//! # Usage
//!
//! ```ignore
//! use baml_tests::bytecode::{Program, ExecState, Value, assert_vm_executes};
//!
//! assert_vm_executes(Program {
//!     source: "function main() -> int { 42 }",
//!     function: "main",
//!     expected: ExecState::Complete(Value::Int(42)),
//! });
//! ```

#![allow(clippy::needless_pass_by_value)] // Test utilities intentionally take ownership

use std::{
    path::PathBuf,
    sync::{Arc, atomic::AtomicU32},
};

use baml_base::{FileId, SourceFile};
use baml_vm::{ObjectIndex, Program as VmProgram, Value as VmValue, Vm, VmExecState};

// Re-export test types from crate::vm
pub use crate::vm::{
    BlockEvent, ExecState, Instance, Instruction, Notification, Object, Value, Variant,
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
pub struct TestDatabase {
    storage: salsa::Storage<Self>,
    next_file_id: Arc<AtomicU32>,
}

#[salsa::db]
impl salsa::Database for TestDatabase {}

#[salsa::db]
impl baml_hir::Db for TestDatabase {}

#[salsa::db]
impl baml_thir::Db for TestDatabase {}

#[salsa::db]
impl baml_mir::Db for TestDatabase {}

impl Default for TestDatabase {
    fn default() -> Self {
        Self::new()
    }
}

impl TestDatabase {
    /// Create a new empty test database.
    pub fn new() -> Self {
        Self {
            storage: salsa::Storage::default(),
            next_file_id: Arc::new(AtomicU32::new(0)),
        }
    }

    /// Add a source file to the database.
    pub fn add_file(&mut self, path: impl Into<PathBuf>, text: impl Into<String>) -> SourceFile {
        let file_id = FileId::new(
            self.next_file_id
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst),
        );
        SourceFile::new(self, text.into(), path.into(), file_id)
    }
}

//
// ────────────────────────────────────────────────────────── COMPILATION ─────
//

/// Compile BAML source code into a VM program.
pub fn compile_source(source: &str) -> VmProgram {
    let mut db = TestDatabase::new();
    let file = db.add_file("test.baml", source);
    baml_codegen::compile_files(&db, &[file])
}

//
// ──────────────────────────────────────────────────── VM TEST UTILITIES ─────
//

/// Helper struct for testing VM execution.
pub struct ProgramInput<Expect> {
    pub source: &'static str,
    pub function: &'static str,
    pub expected: Expect,
}

/// Test input for successful VM execution.
pub type Program = ProgramInput<ExecState>;

/// Test input for VM execution that should fail.
pub type FailingProgram = ProgramInput<baml_vm::errors::VmError>;

/// Test input for VM execution with watch/emit states.
pub type WatchProgram = ProgramInput<Vec<Vec<Notification>>>;

/// Assert that VM execution fails with the expected error.
pub fn assert_vm_fails(input: FailingProgram) -> anyhow::Result<()> {
    assert_vm_fails_with_inspection(input, |_vm| Ok(()))
}

/// Assert that VM execution fails, with access to inspect the VM state.
pub fn assert_vm_fails_with_inspection(
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

/// Assert that VM execution succeeds with the expected result.
#[track_caller]
pub fn assert_vm_executes(input: Program) -> anyhow::Result<()> {
    assert_vm_executes_with_inspection(input, |_vm| Ok(()))
}

/// Assert that VM execution succeeds, with access to inspect the VM state.
#[track_caller]
pub fn assert_vm_executes_with_inspection(
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
pub fn collect_vm_exec_states(
    source: &'static str,
    function: &str,
) -> anyhow::Result<(Vm, Vec<ExecState>)> {
    let program = compile_source(source);

    let function_index = program
        .function_index(function)
        .ok_or_else(|| anyhow::anyhow!("function '{function}' not found"))?;

    let mut vm = Vm::from_program(program);
    vm.set_entry_point(function_index, &[]);

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

/// Assert that VM execution emits the expected watch notifications.
#[track_caller]
pub fn assert_vm_emits(input: WatchProgram) -> anyhow::Result<()> {
    assert_vm_emits_with_inspection(input, |_vm, _states| Ok(()))
}

/// Assert that VM execution emits notifications, with access to inspect the VM state.
#[track_caller]
pub fn assert_vm_emits_with_inspection(
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

    let function_index = program
        .function_index(function)
        .ok_or_else(|| anyhow::anyhow!("function '{function}' not found"))?;

    let mut vm = Vm::from_program(program);
    vm.set_entry_point(function_index, &[]);
    let result = vm.exec();
    Ok((vm, result))
}

//
// ────────────────────────────────────────────────── BYTECODE TEST UTILS ─────
//

/// Helper struct for testing VM execution with direct bytecode.
pub struct BytecodeProgram {
    pub arity: usize,
    pub instructions: Vec<baml_vm::Instruction>,
    pub constants: Vec<VmValue>,
    pub expected: VmExecState,
}

/// Assert that direct bytecode execution succeeds with the expected result.
pub fn assert_vm_executes_bytecode(input: BytecodeProgram) -> anyhow::Result<()> {
    assert_vm_executes_bytecode_with_inspection(input, |_vm, _result| Ok(()))
}

/// Assert that direct bytecode execution succeeds, with access to inspect the VM state.
pub fn assert_vm_executes_bytecode_with_inspection(
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

    let mut program = VmProgram::new();
    let fn_idx = program.add_object(baml_vm::Object::Function(function));
    program.add_global(VmValue::Object(ObjectIndex::from_raw(fn_idx)));
    program
        .function_indices
        .insert("test_fn".to_string(), fn_idx);

    let function_index = program
        .function_index("test_fn")
        .expect("test_fn should exist");

    let mut vm = Vm::from_program(program);
    vm.set_entry_point(function_index, &[]);

    let result = vm.exec()?;

    assert_eq!(
        result, input.expected,
        "VM execution result mismatch for bytecode test",
    );

    inspect(&vm, result)?;

    Ok(())
}
