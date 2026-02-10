//! Canonical schema-level type definitions shared across the compiler pipeline.
//!
//! These types represent classes, enums, functions, and type aliases as they
//! appear in the compiled schema.

use baml_base::Name;

use crate::Ty;

/// Top-level container for all schema definitions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaDefs {
    pub classes: Vec<ClassDef>,
    pub enums: Vec<EnumDef>,
    pub functions: Vec<FunctionDef>,
    pub type_aliases: Vec<TypeAliasDef>,
}

/// A class definition with resolved field types and propagated attributes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassDef {
    pub name: Name,
    pub fields: Vec<FieldDef>,
    pub is_dynamic: bool,
    pub description: Option<String>,
    pub alias: Option<String>,
}

/// A field within a class.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldDef {
    pub name: Name,
    pub ty: Ty,
    pub description: Option<String>,
    pub alias: Option<String>,
    pub skip: bool,
}

/// An enum definition with propagated attributes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumDef {
    pub name: Name,
    pub variants: Vec<EnumVariantDef>,
    pub description: Option<String>,
    pub alias: Option<String>,
}

/// A variant within an enum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumVariantDef {
    pub name: Name,
    pub description: Option<String>,
    pub alias: Option<String>,
    pub skip: bool,
}

/// A function definition with resolved parameter/return types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionDef {
    pub name: Name,
    pub params: Vec<ParamDef>,
    pub return_type: Ty,
    pub body_kind: FunctionBodyKind,
}

/// A function parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParamDef {
    pub name: Name,
    pub ty: Ty,
}

/// The kind of function body — determines how the runtime dispatches it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FunctionBodyKind {
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

/// A type alias definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeAliasDef {
    pub name: Name,
    pub resolves_to: Ty,
}
