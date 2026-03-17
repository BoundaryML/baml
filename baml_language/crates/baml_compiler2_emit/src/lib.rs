//! Code generation for BAML (compiler2 pipeline).
//!
//! Compiles MIR2 to bytecode for the BAML VM using stackification.

mod analysis;
mod emit;
mod pull_semantics;
mod stack_carry;
mod verifier;

pub use analysis::OptLevel;
pub(crate) use emit::compile_mir_function;

use std::collections::HashMap;

use bex_vm_types::ObjectPool;

/// Context for MIR codegen.
pub(crate) struct MirCodegenContext<'ctx, 'obj> {
    pub globals: &'ctx HashMap<String, usize>,
    pub classes: &'ctx HashMap<String, HashMap<String, usize>>,
    pub class_object_indices: &'ctx HashMap<String, usize>,
    pub enum_object_indices: &'ctx HashMap<String, usize>,
    pub enum_variants: &'ctx HashMap<String, HashMap<String, usize>>,
    pub objects: &'obj mut ObjectPool,
}

/// Database trait for compiler2 emit queries.
#[salsa::db]
pub trait Db: baml_compiler2_mir::Db {}

/// Compile options.
pub struct CompileOptions {
    pub emit_test_cases: bool,
}

/// Errors that can occur during bytecode generation.
#[derive(Debug)]
pub enum LoweringError {
    /// A stub — no errors expected from Phase 1 stub.
    Internal(String),
}

impl std::fmt::Display for LoweringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Internal(msg) => write!(f, "internal lowering error: {msg}"),
        }
    }
}

impl std::error::Error for LoweringError {}

pub use bex_vm_types::Program;

/// Generate bytecode for the entire project.
pub fn generate_project_bytecode(
    _db: &dyn baml_compiler2_mir::Db,
    _options: &CompileOptions,
) -> Result<Program, LoweringError> {
    // Stub — will be implemented in Phase 2
    Ok(Program::new())
}
