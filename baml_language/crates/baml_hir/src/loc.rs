//! Location types for interning.
//!
//! Each location uniquely identifies where an item is defined:
//! - File (as a `SourceFile` Salsa input)
//! - Position within that file's `ItemTree`
//!
//! These locations are interned by Salsa to produce compact, stable IDs.
//!
//! We store `SourceFile` (a Salsa input) rather than `FileId` so that
//! queries can directly access the file's `ItemTree` without needing
//! an additional lookup.

use baml_base::SourceFile;

use crate::ids::LocalItemId;

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GeneratorMarker;

/// Location of a function in the source code.
///
/// This gets interned by Salsa to produce a `FunctionId`.
#[salsa::interned]
pub struct FunctionLoc {
    /// File containing this function.
    pub file: SourceFile,

    /// Index in the file's ItemTree.
    pub id: LocalItemId<FunctionMarker>,
}

/// Location of a class definition.
#[salsa::interned]
pub struct ClassLoc {
    pub file: SourceFile,
    pub id: LocalItemId<ClassMarker>,
}

/// Location of an enum definition.
#[salsa::interned]
pub struct EnumLoc {
    pub file: SourceFile,
    pub id: LocalItemId<EnumMarker>,
}

/// Location of a type alias.
#[salsa::interned]
pub struct TypeAliasLoc {
    pub file: SourceFile,
    pub id: LocalItemId<TypeAliasMarker>,
}

/// Location of a client configuration.
#[salsa::interned]
pub struct ClientLoc {
    pub file: SourceFile,
    pub id: LocalItemId<ClientMarker>,
}

/// Location of a test definition.
#[salsa::interned]
pub struct TestLoc {
    pub file: SourceFile,
    pub id: LocalItemId<TestMarker>,
}

/// Location of a generator configuration.
#[salsa::interned]
pub struct GeneratorLoc {
    pub file: SourceFile,
    pub id: LocalItemId<GeneratorMarker>,
}
