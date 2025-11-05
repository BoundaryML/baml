//! Type system for BAML.

use baml_hir::{ClassId, EnumId};

/// A resolved type in BAML.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Ty {
    // Primitive types
    Int,
    Float,
    String,
    Bool,
    Null,

    // User-defined types
    Class(ClassId),
    Enum(EnumId),

    // Type constructors
    Optional(Box<Ty>),
    List(Box<Ty>),
    Map { key: Box<Ty>, value: Box<Ty> },
    Union(Vec<Ty>),

    // Special types
    Unknown,
    Error,
}

impl Ty {
    /// Check if this type is an error type.
    pub fn is_error(&self) -> bool {
        matches!(self, Ty::Error)
    }

    /// Check if this type is unknown.
    pub fn is_unknown(&self) -> bool {
        matches!(self, Ty::Unknown)
    }

    /// Make this type optional.
    #[must_use]
    pub fn into_optional(self) -> Self {
        Ty::Optional(Box::new(self))
    }

    /// Make a list of this type.
    #[must_use]
    pub fn into_list(self) -> Self {
        Ty::List(Box::new(self))
    }
}
