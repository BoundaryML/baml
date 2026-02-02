//! Schema items for VIR.
//!
//! VIR schema captures classes, enums, and functions with resolved types
//! and propagated HIR attributes. Emit maps these to `bex_program` types.

use baml_base::Name;
use baml_type::Ty;

/// Top-level schema container produced by `project_schema`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirSchema {
    pub classes: Vec<VirClass>,
    pub enums: Vec<VirEnum>,
    pub functions: Vec<VirFunction>,
}

/// A class definition with resolved field types and propagated attributes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirClass {
    pub name: Name,
    pub fields: Vec<VirField>,
    /// @@dynamic — marks class as dynamically extensible
    pub is_dynamic: bool,
    /// @@description("text")
    pub description: Option<String>,
    /// @@alias("name")
    pub alias: Option<String>,
}

/// A field within a class.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirField {
    pub name: Name,
    /// Resolved type (`baml_type::Ty`, aliases expanded, literals preserved)
    pub ty: Ty,
    /// @description("text")
    pub description: Option<String>,
    /// @alias("name")
    pub alias: Option<String>,
    /// @skip — exclude field from serialization
    pub skip: bool,
}

/// An enum definition with propagated attributes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirEnum {
    pub name: Name,
    pub variants: Vec<VirEnumVariant>,
    /// @@description("text")
    pub description: Option<String>,
    /// @@alias("name")
    pub alias: Option<String>,
}

/// A variant within an enum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirEnumVariant {
    pub name: Name,
    /// @description("text")
    pub description: Option<String>,
    /// @alias("name")
    pub alias: Option<String>,
    /// @skip — exclude variant from serialization
    pub skip: bool,
}

/// A function definition with resolved parameter/return types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirFunction {
    pub name: Name,
    pub params: Vec<VirParam>,
    pub return_type: Ty,
    pub body_kind: VirFunctionBodyKind,
}

/// A function parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirParam {
    pub name: Name,
    pub ty: Ty,
}

/// The kind of function body — determines how the runtime dispatches it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VirFunctionBodyKind {
    /// Declarative LLM function with prompt template and client reference.
    Llm {
        prompt_template: String,
        client: String,
    },
    /// Imperative expression function — compiled to bytecode.
    Expr,
    /// Function body is missing (error recovery in HIR).
    Missing,
}
