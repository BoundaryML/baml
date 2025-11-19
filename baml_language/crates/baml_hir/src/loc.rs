//! Location types for interning.
//!
//! Each location uniquely identifies where an item is defined:
//! - File (current implementation)
//! - Position within that file's `ItemTree`
//!
//! These locations are interned by Salsa to produce compact, stable IDs.
//!
//! Note: We use `FileId` directly instead of `ContainerId` for now to avoid
//! Salsa complications with non-Copy enums. When we add modules, we'll
//! need to refactor this.

use crate::ids::LocalItemId;
use baml_base::FileId;

/// Marker types for different item kinds in the `ItemTree`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FunctionMarker;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClassMarker;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EnumMarker;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeAliasMarker;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClientMarker;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TestMarker;

/// Location of a function in the source code.
///
/// This gets interned by Salsa to produce a `FunctionId`.
#[salsa::interned]
pub struct FunctionLoc {
    /// File containing this function.
    pub file: FileId,

    /// Index in the file's ItemTree.
    pub id: LocalItemId<FunctionMarker>,
}

/// Location of a class definition.
#[salsa::interned]
pub struct ClassLoc {
    pub file: FileId,
    pub id: LocalItemId<ClassMarker>,
}

/// Location of an enum definition.
#[salsa::interned]
pub struct EnumLoc {
    pub file: FileId,
    pub id: LocalItemId<EnumMarker>,
}

/// Location of a type alias.
#[salsa::interned]
pub struct TypeAliasLoc {
    pub file: FileId,
    pub id: LocalItemId<TypeAliasMarker>,
}

/// Location of a client configuration.
#[salsa::interned]
pub struct ClientLoc {
    pub file: FileId,
    pub id: LocalItemId<ClientMarker>,
}

/// Location of a test definition.
#[salsa::interned]
pub struct TestLoc {
    pub file: FileId,
    pub id: LocalItemId<TestMarker>,
}
