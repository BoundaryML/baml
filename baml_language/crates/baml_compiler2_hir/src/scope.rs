//! Scope tree data structures for `compiler2_hir`.
//!
//! Scopes are allocated in DFS pre-order during `SemanticIndexBuilder::build`.
//! Each `Scope` carries a `TextRange` for `scope_at_offset()`.
//! `ScopeId<'db>` is a Salsa tracked struct enabling per-scope queries.

use std::ops::Range;

use baml_base::{Name, SourceFile};
use text_size::TextRange;

/// Dense sequential index into the per-file scope arena.
/// `FileScopeId(0)` is always the Project scope (outermost).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FileScopeId(u32);

impl FileScopeId {
    pub const ROOT: FileScopeId = FileScopeId(0);

    pub fn new(index: u32) -> Self {
        Self(index)
    }

    pub fn index(self) -> u32 {
        self.0
    }

    #[must_use]
    pub fn next(self) -> Self {
        Self(self.0 + 1)
    }

    /// Convert to a cross-file Salsa identity.
    pub fn to_scope_id(self, db: &dyn crate::Db, file: SourceFile) -> ScopeId<'_> {
        ScopeId::new(db, file, self)
    }
}

/// Cross-file scope identity — used as a Salsa query key for per-scope
/// queries like `infer_scope_types(db, scope_id)`.
///
/// Modeled after Ty's `ScopeId<'db>` which is also `#[salsa::tracked]`
/// pairing File + FileScopeId.
#[salsa::tracked]
pub struct ScopeId<'db> {
    pub file: SourceFile,
    pub file_scope_id: FileScopeId,
}

/// What kind of scope this is in the hierarchy.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ScopeKind {
    /// The compilation unit — collects all packages.
    Project,
    /// A unit of code with its own root (user, baml, env, ...).
    Package,
    /// A named subdivision within a package — can nest.
    Namespace,
    /// A .baml file — child of Package or innermost Namespace.
    File,
    /// Class body (fields + methods).
    Class,
    /// Enum body (variants).
    Enum,
    /// Function body.
    Function,
    /// Type alias RHS.
    TypeAlias,
    /// Block expression with let bindings.
    Block,
    /// Lambda expression body — own scope for per-scope incremental inference.
    Lambda,
    /// Client, test, generator, template string, retry policy body.
    Item,
}

/// A single scope node in the per-file scope tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scope {
    /// Parent scope. `None` only for the Project root scope.
    pub parent: Option<FileScopeId>,
    /// What kind of scope this is.
    pub kind: ScopeKind,
    /// Optional name (packages, namespaces, items have names; blocks don't).
    pub name: Option<Name>,
    /// Source range of this scope. Used by `scope_at_offset()` to find the
    /// innermost scope containing a cursor position. Structural scopes
    /// (Project, Package, Namespace) use the file's full range.
    pub range: TextRange,
    /// Contiguous range of descendant scope IDs (DFS pre-order).
    /// All scopes in `descendants` are proper descendants of this scope.
    pub descendants: Range<FileScopeId>,
}
