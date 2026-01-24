//! BAML Snapshot representation.
//!
//! This crate defines `BamlSnapshot`, the final compiled artifact that contains
//! everything needed to execute BAML code at runtime:
//! - Type definitions (classes, enums)
//! - Function definitions (LLM and expression functions)
//! - Infrastructure (clients, retry policies)
//! - Bytecode for VM execution

use std::collections::HashMap;

pub use bex_vm_types::Program;

// ============================================================================
// Type System
// ============================================================================

/// The type representation used throughout the compiled program.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Ty {
    // Primitives
    Int,
    Float,
    String,
    Bool,
    Null,

    // Media types
    Media(MediaKind),

    // Literal types
    Literal(LiteralValue),

    // Named types (references into BamlSnapshot.classes/enums)
    Class(String),
    Enum(String),

    // Container types
    Optional(Box<Ty>),
    List(Box<Ty>),
    Map { key: Box<Ty>, value: Box<Ty> },
    Union(Vec<Ty>),
}

/// Media type variants.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum MediaKind {
    Image,
    Audio,
    Video,
    Pdf,
}

/// Literal value types for literal types.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum LiteralValue {
    String(String),
    Int(i64),
    Bool(bool),
}

// ============================================================================
// Type Definitions
// ============================================================================

/// A class definition.
#[derive(Clone, Debug)]
pub struct ClassDef {
    pub name: String,
    pub fields: Vec<FieldDef>,
    pub description: Option<String>,
}

/// A field within a class.
#[derive(Clone, Debug)]
pub struct FieldDef {
    pub name: String,
    pub field_type: Ty,
    pub description: Option<String>,
    pub alias: Option<String>,
}

/// An enum definition.
#[derive(Clone, Debug)]
pub struct EnumDef {
    pub name: String,
    pub variants: Vec<EnumVariantDef>,
    pub description: Option<String>,
}

/// A variant within an enum.
#[derive(Clone, Debug)]
pub struct EnumVariantDef {
    pub name: String,
    pub description: Option<String>,
    pub alias: Option<String>,
}

// ============================================================================
// Function Definitions
// ============================================================================

/// A function definition.
#[derive(Clone, Debug)]
pub struct FunctionDef {
    pub name: String,
    pub params: Vec<ParamDef>,
    pub return_type: Ty,
    pub body: FunctionBody,
}

/// A function parameter.
#[derive(Clone, Debug)]
pub struct ParamDef {
    pub name: String,
    pub param_type: Ty,
}

/// The body of a function - either LLM or expression.
#[derive(Clone, Debug)]
pub enum FunctionBody {
    /// Declarative LLM function - prompt template + client config.
    Llm {
        prompt_template: String,
        client: String,
    },
    /// Imperative expression function - compiled to bytecode.
    Expr {
        /// Index into the bytecode program's function table.
        bytecode_index: usize,
    },
}

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
pub struct BamlSnapshot {
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

impl BamlSnapshot {
    /// Create a new `BamlSnapshot` with the given bytecode program.
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
}
