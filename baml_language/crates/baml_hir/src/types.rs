//! HIR type definitions.

use baml_base::Name;

/// Function data in HIR.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FunctionData {
    pub name: Name,
    pub params: Vec<Parameter>,
    pub return_type: TypeRef,
}

/// Function parameter.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Parameter {
    pub name: Name,
    pub ty: TypeRef,
}

/// Class data in HIR.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ClassData {
    pub name: Name,
    pub fields: Vec<Field>,
}

/// Class field.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Field {
    pub name: Name,
    pub ty: TypeRef,
    pub optional: bool,
}

/// Type reference (unresolved at HIR level).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TypeRef {
    Named(Name),
    Optional(Box<TypeRef>),
    List(Box<TypeRef>),
    Union(Vec<TypeRef>),
    Unknown,
}
