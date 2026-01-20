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
// Re-export BamlSnapshot for engine tests
pub use baml_snapshot::BamlSnapshot;
use bex_vm::{BexVm, VmExecState};
use bex_vm_types::{ObjectIndex, Program as VmProgram, Value as VmValue};

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

    /// Set the project with the given files.
    pub fn set_project(&mut self, files: Vec<SourceFile>) {
        let project = baml_workspace::Project::new(self, PathBuf::new(), files);
        self.project = Some(project);
    }
}

//
// ────────────────────────────────────────────────────────── COMPILATION ─────
//

/// Compile BAML source code into a VM program.
pub fn compile_source(source: &str) -> VmProgram<()> {
    let mut db = TestDatabase::new();
    let file = db.add_file("test.baml", source);
    db.set_project(vec![file]);
    baml_compiler_emit::compile_files(&db, &[file])
        .expect("compile_files should succeed for valid test source")
}

/// Compile BAML source code into a `BamlSnapshot` with schema populated.
///
/// This function extracts function return types from the TIR so that
/// `BamlSnapshot.functions` is properly populated for engine tests.
pub fn compile_source_with_schema(source: &str) -> BamlSnapshot {
    use std::collections::HashMap;

    use baml_compiler_hir::{ItemId, file_item_tree, function_signature};
    use baml_compiler_tir::TypeResolutionContext;
    use baml_workspace::Db as _;

    let mut db = TestDatabase::new();
    let file = db.add_file("test.baml", source);
    db.set_project(vec![file]);

    // Compile to bytecode
    let bytecode = baml_compiler_emit::compile_files(&db, &[file])
        .expect("compile_files should succeed for valid test source");

    // Build typing context to lower TypeRefs to Tys
    let project = db.project();
    let resolution_ctx = TypeResolutionContext::new(&db, project);

    // Get item tree for accessing class/enum definitions
    let item_tree = file_item_tree(&db, file);

    let mut functions = HashMap::new();
    let mut classes = HashMap::new();
    let mut enums = HashMap::new();

    let items_struct = baml_compiler_hir::file_items(&db, file);
    for item in items_struct.items(&db) {
        match item {
            ItemId::Function(func_loc) => {
                let signature = function_signature(&db, *func_loc);

                // Lower return type from TypeRef to TIR Ty
                let (tir_return_type, _) = resolution_ctx
                    .lower_type_ref(&signature.return_type, baml_base::Span::default());

                // Convert TIR Ty to Snapshot Ty
                let return_type = convert_tir_ty_to_snapshot_ty(&tir_return_type);

                // Build params
                let params: Vec<baml_snapshot::ParamDef> = signature
                    .params
                    .iter()
                    .map(|p| {
                        let (tir_ty, _) =
                            resolution_ctx.lower_type_ref(&p.type_ref, baml_base::Span::default());
                        baml_snapshot::ParamDef {
                            name: p.name.to_string(),
                            param_type: convert_tir_ty_to_snapshot_ty(&tir_ty),
                        }
                    })
                    .collect();

                let func_def = baml_snapshot::FunctionDef {
                    name: signature.name.to_string(),
                    params,
                    return_type,
                    body: baml_snapshot::FunctionBody::Expr {
                        bytecode_index: 0, // Not needed for type checking
                    },
                };

                functions.insert(signature.name.to_string(), func_def);
            }
            ItemId::Class(class_loc) => {
                let class = &item_tree[class_loc.id(&db)];
                let class_name = class.name.to_string();

                let fields: Vec<baml_snapshot::FieldDef> = class
                    .fields
                    .iter()
                    .map(|field| {
                        let (tir_ty, _) = resolution_ctx
                            .lower_type_ref(&field.type_ref, baml_base::Span::default());
                        baml_snapshot::FieldDef {
                            name: field.name.to_string(),
                            field_type: convert_tir_ty_to_snapshot_ty(&tir_ty),
                            description: None,
                            alias: None,
                        }
                    })
                    .collect();

                let class_def = baml_snapshot::ClassDef {
                    name: class_name.clone(),
                    fields,
                    description: None,
                };

                classes.insert(class_name, class_def);
            }
            ItemId::Enum(enum_loc) => {
                let enum_def = &item_tree[enum_loc.id(&db)];
                let enum_name = enum_def.name.to_string();

                let variants: Vec<baml_snapshot::EnumVariantDef> = enum_def
                    .variants
                    .iter()
                    .map(|variant| baml_snapshot::EnumVariantDef {
                        name: variant.name.to_string(),
                        description: None,
                        alias: None,
                    })
                    .collect();

                let enum_def = baml_snapshot::EnumDef {
                    name: enum_name.clone(),
                    variants,
                    description: None,
                };

                enums.insert(enum_name, enum_def);
            }
            _ => {}
        }
    }

    BamlSnapshot {
        classes,
        enums,
        functions,
        clients: HashMap::new(),
        retry_policies: HashMap::new(),
        bytecode,
    }
}

/// Convert a TIR `Ty` to a Snapshot `Ty`.
///
/// The main difference is that TIR uses `FullyQualifiedName` for classes/enums,
/// while Snapshot uses plain `String`.
fn convert_tir_ty_to_snapshot_ty(tir_ty: &baml_compiler_tir::Ty) -> baml_snapshot::Ty {
    use baml_compiler_tir::Ty as TirTy;
    use baml_snapshot::Ty as SnapTy;

    match tir_ty {
        TirTy::Int => SnapTy::Int,
        TirTy::Float => SnapTy::Float,
        TirTy::String => SnapTy::String,
        TirTy::Bool => SnapTy::Bool,
        TirTy::Null => SnapTy::Null,

        TirTy::Media(kind) => {
            let snap_kind = match kind {
                baml_base::MediaKind::Image => baml_snapshot::MediaKind::Image,
                baml_base::MediaKind::Audio => baml_snapshot::MediaKind::Audio,
                baml_base::MediaKind::Video => baml_snapshot::MediaKind::Video,
                baml_base::MediaKind::Pdf => baml_snapshot::MediaKind::Pdf,
                baml_base::MediaKind::Generic => baml_snapshot::MediaKind::Image,
            };
            SnapTy::Media(snap_kind)
        }

        TirTy::Literal(val) => {
            let snap_val = match val {
                baml_compiler_tir::LiteralValue::Int(i) => baml_snapshot::LiteralValue::Int(*i),
                baml_compiler_tir::LiteralValue::Float(s) => {
                    baml_snapshot::LiteralValue::Int(s.parse().unwrap_or(0))
                }
                baml_compiler_tir::LiteralValue::String(s) => {
                    baml_snapshot::LiteralValue::String(s.clone())
                }
                baml_compiler_tir::LiteralValue::Bool(b) => baml_snapshot::LiteralValue::Bool(*b),
            };
            SnapTy::Literal(snap_val)
        }

        TirTy::Class(fqn) => SnapTy::Class(fqn.to_string()),
        TirTy::Enum(fqn) => SnapTy::Enum(fqn.to_string()),
        TirTy::TypeAlias(fqn) => SnapTy::Class(fqn.to_string()),

        TirTy::Optional(inner) => SnapTy::Optional(Box::new(convert_tir_ty_to_snapshot_ty(inner))),
        TirTy::List(inner) => SnapTy::List(Box::new(convert_tir_ty_to_snapshot_ty(inner))),
        TirTy::Map { key, value } => SnapTy::Map {
            key: Box::new(convert_tir_ty_to_snapshot_ty(key)),
            value: Box::new(convert_tir_ty_to_snapshot_ty(value)),
        },
        TirTy::Union(types) => {
            SnapTy::Union(types.iter().map(convert_tir_ty_to_snapshot_ty).collect())
        }

        TirTy::Function { params, ret } => {
            let _ = (params, ret);
            SnapTy::Null
        }

        TirTy::Unknown | TirTy::Error | TirTy::Void => SnapTy::Null,
        TirTy::WatchAccessor(inner) => convert_tir_ty_to_snapshot_ty(inner),
        TirTy::Builtin(path) => SnapTy::Class(path.clone()),
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
    pub instructions: Vec<bex_vm_types::Instruction>,
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
    };

    let mut program = VmProgram::new();
    let fn_idx = program.add_object(bex_vm_types::Object::Function(function));
    program.add_global(VmValue::Object(ObjectIndex::from_raw(fn_idx)));
    program
        .function_indices
        .insert("test_fn".to_string(), fn_idx);

    let function_index = program
        .function_index("test_fn")
        .expect("test_fn should exist");

    let mut vm = BexVm::from_program(program)?;
    vm.set_entry_point(function_index, &[]);

    let result = vm.exec()?;

    assert_eq!(
        result, input.expected,
        "VM execution result mismatch for bytecode test",
    );

    inspect(&vm, result)?;

    Ok(())
}
