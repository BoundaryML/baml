//! Shared test utilities for BAML bytecode testing.
//!
//! This module provides common infrastructure for testing bytecode compilation
//! and execution in `bex_vm`.
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
// Re-export BexProgram for engine tests
pub use bex_program::BexProgram;
use bex_vm::{BexVm, VmExecState};
use bex_vm_types::{ConstValue, ObjectIndex, Program as VmProgram};

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
/// `baml_compiler_emit` and `baml_db`.
#[salsa::db]
#[derive(Clone)]
pub struct TestDatabase {
    storage: salsa::Storage<Self>,
    next_file_id: Arc<AtomicU32>,
    project: Option<baml_workspace::Project>,
}

#[salsa::db]
impl salsa::Database for TestDatabase {}

#[salsa::db]
impl baml_workspace::Db for TestDatabase {
    fn project(&self) -> baml_workspace::Project {
        self.project.expect("project must be set before querying")
    }
}

#[salsa::db]
impl baml_compiler_hir::Db for TestDatabase {}

#[salsa::db]
impl baml_compiler_tir::Db for TestDatabase {}

#[salsa::db]
impl baml_compiler_vir::Db for TestDatabase {}

#[salsa::db]
impl baml_compiler_mir::Db for TestDatabase {}

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
            project: None,
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

    /// Load builtin BAML source files (like llm.baml) into the database.
    fn load_builtin_files(&mut self) -> Vec<SourceFile> {
        baml_builtins::baml_sources()
            .map(|src| self.add_file(src.path, src.source))
            .collect()
    }

    /// Set the project with the given files, including builtin BAML files.
    ///
    /// Builtin files are added after user files to match the order used in
    /// production (user files first, builtins appended).
    pub fn set_project(&mut self, user_files: Vec<SourceFile>) {
        // User files first, then builtin files (matches production order)
        let builtin_files = self.load_builtin_files();
        let mut all_files = user_files;
        all_files.extend(builtin_files);
        let project = baml_workspace::Project::new(self, PathBuf::new(), all_files);
        self.project = Some(project);
    }
}

//
// ────────────────────────────────────────────────────────── COMPILATION ─────
//

/// Compile BAML source code into a VM program.
pub fn compile_source(source: &str) -> VmProgram {
    use baml_workspace::Db as _;

    let mut db = TestDatabase::new();
    let file = db.add_file("test.baml", source);
    db.set_project(vec![file]);
    // Pass all project files (user + builtin) to compile_files
    baml_compiler_emit::compile_files(&db, db.project().files(&db))
        .expect("compile_files should succeed for valid test source")
}

/// Compile BAML source code into a `BexProgram` with schema populated.
///
/// This function uses VIR schema to populate the BexProgram schema types
/// for engine tests.
pub fn compile_source_with_schema(source: &str) -> BexProgram {
    use std::collections::HashMap;

    use baml_workspace::Db as _;

    let mut db = TestDatabase::new();
    let file = db.add_file("test.baml", source);
    db.set_project(vec![file]);

    // Compile to bytecode - pass all project files (user + builtin)
    let bytecode = baml_compiler_emit::compile_files(&db, db.project().files(&db))
        .expect("compile_files should succeed for valid test source");

    // Get VIR schema
    let project = db.project();
    let schema = baml_compiler_vir::project_schema(&db, project);

    // Map VIR schema to bex_program types (inline mapping since we can't depend on bridge)
    let classes = schema
        .classes
        .iter()
        .map(|c| {
            let fields = c
                .fields
                .iter()
                .filter(|f| !f.skip) // Filter out @skip fields
                .map(|f| bex_program::FieldDef {
                    name: f.name.to_string(),
                    field_type: f.ty.clone(),
                    description: f.description.clone(),
                    alias: f.alias.clone(),
                })
                .collect();
            (
                c.name.to_string(),
                bex_program::ClassDef {
                    name: c.name.to_string(),
                    fields,
                    description: c.description.clone(),
                },
            )
        })
        .collect();

    let enums = schema
        .enums
        .iter()
        .map(|e| {
            let variants = e
                .variants
                .iter()
                .map(|v| bex_program::EnumVariantDef {
                    name: v.name.to_string(),
                    description: v.description.clone(),
                    alias: v.alias.clone(),
                    skip: v.skip,
                })
                .collect();
            (
                e.name.to_string(),
                bex_program::EnumDef {
                    name: e.name.to_string(),
                    variants,
                    description: e.description.clone(),
                },
            )
        })
        .collect();

    let functions = schema
        .functions
        .iter()
        .map(|f| {
            let params = f
                .params
                .iter()
                .map(|p| bex_program::ParamDef {
                    name: p.name.to_string(),
                    param_type: p.ty.clone(),
                })
                .collect();

            let body = match &f.body_kind {
                baml_compiler_vir::VirFunctionBodyKind::Llm {
                    prompt_template,
                    client,
                } => bex_program::FunctionBody::Llm {
                    prompt_template: prompt_template.clone(),
                    client: client.clone(),
                },
                baml_compiler_vir::VirFunctionBodyKind::Expr
                | baml_compiler_vir::VirFunctionBodyKind::Missing => {
                    bex_program::FunctionBody::Expr
                }
            };

            (
                f.name.to_string(),
                bex_program::FunctionDef {
                    name: f.name.to_string(),
                    params,
                    return_type: f.return_type.clone(),
                    body,
                },
            )
        })
        .collect();

    BexProgram {
        classes,
        enums,
        functions,
        clients: HashMap::new(),
        retry_policies: HashMap::new(),
        bytecode,
    }
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
pub type FailingProgram = ProgramInput<bex_vm::errors::VmError>;

/// Test input for VM execution with watch/emit states.
pub type WatchProgram = ProgramInput<Vec<Vec<Notification>>>;

/// Assert that VM execution fails with the expected error.
pub fn assert_vm_fails(input: FailingProgram) -> anyhow::Result<()> {
    assert_vm_fails_with_inspection(input, |_vm| Ok(()))
}

/// Assert that VM execution fails, with access to inspect the VM state.
pub fn assert_vm_fails_with_inspection(
    input: FailingProgram,
    inspect: impl FnOnce(&BexVm) -> anyhow::Result<()>,
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
    inspect: impl FnOnce(&BexVm) -> anyhow::Result<()>,
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
) -> anyhow::Result<(BexVm, Vec<ExecState>)> {
    let program = compile_source(source);

    let function_index = program
        .function_index(function)
        .ok_or_else(|| anyhow::anyhow!("function '{function}' not found"))?;

    let mut vm = BexVm::from_program(program)?;
    let function_ptr = vm.heap.compile_time_ptr(function_index);
    vm.set_entry_point(function_ptr, &[]);

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
    inspect: impl FnOnce(&BexVm, &[ExecState]) -> anyhow::Result<()>,
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
) -> Result<(BexVm, Result<VmExecState, bex_vm::errors::VmError>), anyhow::Error> {
    let program = compile_source(source);

    let function_index = program
        .function_index(function)
        .ok_or_else(|| anyhow::anyhow!("function '{function}' not found"))?;

    let mut vm = BexVm::from_program(program)?;
    let function_ptr = vm.heap.compile_time_ptr(function_index);
    vm.set_entry_point(function_ptr, &[]);
    let result = vm.exec();
    Ok((vm, result))
}

//
// ────────────────────────────────────────────────── BYTECODE TEST UTILS ─────
//

/// Helper struct for testing VM execution with direct bytecode.
pub struct BytecodeProgram {
    pub arity: usize,
    pub instructions: Vec<bex_vm_types::Instruction>,
    pub constants: Vec<ConstValue>,
    pub expected: VmExecState,
}

/// Assert that direct bytecode execution succeeds with the expected result.
pub fn assert_vm_executes_bytecode(input: BytecodeProgram) -> anyhow::Result<()> {
    assert_vm_executes_bytecode_with_inspection(input, |_vm, _result| Ok(()))
}

/// Assert that direct bytecode execution succeeds, with access to inspect the VM state.
pub fn assert_vm_executes_bytecode_with_inspection(
    input: BytecodeProgram,
    inspect: impl FnOnce(&BexVm, VmExecState) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let function = bex_vm_types::Function {
        name: "test_fn".to_string(),
        arity: input.arity,
        bytecode: bex_vm_types::Bytecode {
            source_lines: vec![1; input.instructions.len()],
            scopes: vec![0; input.instructions.len()],
            instructions: input.instructions,
            constants: input.constants,
            resolved_constants: Vec::new(), // Populated by BexHeap at load time
            jump_tables: Vec::new(),
        },
        kind: bex_vm_types::FunctionKind::Bytecode,
        locals_in_scope: {
            let mut names = Vec::with_capacity(input.arity + 1);
            names.push("<fn test_fn>".to_string());
            names.resize_with(names.capacity(), String::new);
            vec![names]
        },
        span: baml_base::Span::fake(),
        block_notifications: Vec::new(),
        viz_nodes: Vec::new(),
        return_type: baml_type::Ty::Null,
        param_names: Vec::new(),
        param_types: Vec::new(),
        body_meta: None,
    };

    let mut program = VmProgram::new();
    let fn_idx = program.add_object(bex_vm_types::Object::Function(Box::new(function)));
    program.add_global(ConstValue::Object(ObjectIndex::from_raw(fn_idx)));
    program
        .function_indices
        .insert("test_fn".to_string(), fn_idx);

    let mut vm = BexVm::from_program(program)?;
    // Get HeapPtr for function from the heap
    let function_ptr = vm.heap.compile_time_ptr(fn_idx);
    vm.set_entry_point(function_ptr, &[]);

    let result = vm.exec()?;

    assert_eq!(
        result, input.expected,
        "VM execution result mismatch for bytecode test",
    );

    inspect(&vm, result)?;

    Ok(())
}
