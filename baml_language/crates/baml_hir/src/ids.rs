//! HIR item identifiers.

use baml_base::{Name, SourceFile};

/// A function in the HIR.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FunctionId {
    pub file: SourceFile,
    pub name: Name,
}

/// A class in the HIR.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ClassId {
    pub file: SourceFile,
    pub name: Name,
}

/// An enum in the HIR.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EnumId {
    pub file: SourceFile,
    pub name: Name,
}

/// Any top-level item.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ItemId {
    Function(FunctionId),
    Class(ClassId),
    Enum(EnumId),
}
