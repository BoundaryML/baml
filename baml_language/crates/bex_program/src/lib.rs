//! BAML Snapshot representation.
//!
//! This crate defines `BexProgram`, the final compiled artifact that contains
//! everything needed to execute BAML code at runtime:
//! - Type definitions (classes, enums)
//! - Function definitions (LLM and expression functions)
//! - Infrastructure (clients, retry policies)
//! - Bytecode for VM execution

use std::collections::HashMap;

pub use baml_base::{Literal as LiteralValue, MediaKind};
// ============================================================================
// Type System
// ============================================================================
/// The unified type representation from `baml_type`.
pub use baml_type::Ty;
pub use baml_type::TypeName;
// ============================================================================
// Type Definitions (re-exported from baml_type)
// ============================================================================
pub use baml_type::{
    ClassDef, EnumDef, EnumVariantDef, FieldDef, FunctionBodyKind as FunctionBody, FunctionDef,
    ParamDef, TypeAliasDef,
};
pub use bex_vm_types::Program;

// ============================================================================
// Infrastructure Definitions
// ============================================================================

/// A client definition (LLM provider configuration).
#[derive(Clone, Debug)]
pub struct ClientDef {
    pub name: String,
    pub provider: String,
    pub options: HashMap<String, String>,
    pub retry_policy: Option<String>,
}

/// A retry policy definition.
#[derive(Clone, Debug)]
pub struct RetryPolicyDef {
    pub name: String,
    pub max_retries: u32,
    pub strategy: RetryStrategy,
}

/// Retry strategy variants.
#[derive(Clone, Debug)]
pub enum RetryStrategy {
    ExponentialBackoff {
        initial_delay_ms: u64,
        multiplier: f64,
        max_delay_ms: u64,
    },
    ConstantDelay {
        delay_ms: u64,
    },
}

// ============================================================================
// Program Structure
// ============================================================================

/// The compiled BAML program - everything needed for runtime execution.
///
/// This is the final artifact produced by the compiler, containing:
/// - Type definitions (classes, enums)
/// - Function definitions (LLM and expression functions)
/// - Infrastructure (clients, retry policies)
/// - Bytecode for VM execution
#[derive(Clone, Debug)]
pub struct BexProgram {
    /// Class definitions, keyed by name.
    pub classes: HashMap<String, ClassDef>,

    /// Enum definitions, keyed by name.
    pub enums: HashMap<String, EnumDef>,

    /// Function definitions, keyed by name.
    pub functions: HashMap<String, FunctionDef>,

    /// Client definitions, keyed by name.
    pub clients: HashMap<String, ClientDef>,

    /// Retry policy definitions, keyed by name.
    pub retry_policies: HashMap<String, RetryPolicyDef>,

    /// Bytecode program for VM execution (pure data, no native functions attached).
    pub bytecode: Program,
}

impl BexProgram {
    /// Create a new `BexProgram` with the given bytecode program.
    pub fn new(bytecode: Program) -> Self {
        Self {
            classes: HashMap::new(),
            enums: HashMap::new(),
            functions: HashMap::new(),
            clients: HashMap::new(),
            retry_policies: HashMap::new(),
            bytecode,
        }
    }

    /// Validate that no compiler-only type variants appear in runtime-facing types.
    /// Returns Ok(()) if all types are valid for runtime, or Err with a description.
    pub fn validate(&self) -> Result<(), String> {
        for (class_name, class_def) in &self.classes {
            for field in &class_def.fields {
                field
                    .ty
                    .validate_runtime()
                    .map_err(|e| format!("Class '{class_name}' field '{}': {e}", field.name))?;
            }
        }
        for (fn_name, fn_def) in &self.functions {
            fn_def
                .return_type
                .validate_runtime()
                .map_err(|e| format!("Function '{fn_name}' return type: {e}"))?;
            for param in &fn_def.params {
                param
                    .ty
                    .validate_runtime()
                    .map_err(|e| format!("Function '{fn_name}' param '{}': {e}", param.name))?;
            }
        }
        Ok(())
    }
}
