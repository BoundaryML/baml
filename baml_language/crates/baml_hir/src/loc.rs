//! Location types for interning.
//!
//! Each location uniquely identifies where an item is defined:
//! - `SourceFile` (Salsa input tracking the file content)
//! - Position within that file's `ItemTree`
//! - `AstId` for stable syntax node addressing (functions only, for now)
//!
//! These locations are interned by Salsa to produce compact, stable IDs.
//!
//! Note: We use `SourceFile` directly instead of `ContainerId` for now to avoid
//! complexity. When we add modules, we'll need to refactor this.

use crate::{FileAstId, ids::LocalItemId};
use baml_base::SourceFile;

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
    /// Note: This was previously FileId, but changed to SourceFile to enable
    /// proper incremental compilation. Salsa can track changes to SourceFile
    /// and only re-run queries when the actual file content changes.
    pub file: SourceFile,

    /// Index in the file's ItemTree.
    pub id: LocalItemId<FunctionMarker>,

    /// Stable pointer into syntax tree for direct node access.
    /// This enables O(1) lookup of the function's syntax node without
    /// walking the entire CST.
    pub ast_id: FileAstId,
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
