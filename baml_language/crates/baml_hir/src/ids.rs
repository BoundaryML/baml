//! HIR item identifiers.

use baml_base::{FileId, Name};

/// A function in the HIR.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FunctionId {
    pub file: FileId,
    pub name: Name,
}

/// A class in the HIR.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ClassId {
    pub file: FileId,
    pub name: Name,
}

/// An enum in the HIR.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EnumId {
    pub file: FileId,
    pub name: Name,
}

/// Any top-level item.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ItemId {
    Function(FunctionId),
    Class(ClassId),
    Enum(EnumId),
}
