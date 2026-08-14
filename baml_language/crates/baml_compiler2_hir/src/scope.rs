//! Scope tree data structures for `compiler2_hir`.
//!
//! Scopes are allocated in DFS pre-order during `SemanticIndexBuilder::build`.
//! Each `Scope` carries a `TextRange` for `scope_at_offset()`.
//! `ScopeId<'db>` is a Salsa tracked struct enabling per-scope queries.

use std::ops::Range;

use baml_base::{Name, SourceFile};
use text_size::TextRange;

use crate::ids::{
    ClassMarker, ClientMarker, EnumMarker, FunctionMarker, ImplMarker, InterfaceMarker, LetMarker,
    LocalItemId, RetryPolicyMarker, TemplateStringMarker, TestMarker, TypeAliasMarker,
};

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
    /// Client, test, template string, retry policy body.
    Item,
    /// Match arm body — holds pattern bindings visible to the arm body and guard.
    MatchArm,
    /// Catch clause — wraps all arms of a catch clause, holds the clause-level binding.
    CatchClause,
    /// Catch arm body — holds arm-level pattern bindings.
    CatchArm,
    /// Top-level let binding — owns an initializer expression.
    Let,
}

/// The item a scope was opened for.
///
/// The builder creates an item's scope in the same step that allocates the item,
/// so this link is recorded directly rather than reconstructed. Before it
/// existed, ~20 sites across TIR/MIR/LSP recovered it by comparing
/// `item.span == scope.range` — which made item spans load-bearing *semantic
/// identity* and blocked moving them into the source map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ItemScopeOwner {
    Function(LocalItemId<FunctionMarker>),
    Class(LocalItemId<ClassMarker>),
    Enum(LocalItemId<EnumMarker>),
    Interface(LocalItemId<InterfaceMarker>),
    TypeAlias(LocalItemId<TypeAliasMarker>),
    TemplateString(LocalItemId<TemplateStringMarker>),
    Client(LocalItemId<ClientMarker>),
    Test(LocalItemId<TestMarker>),
    RetryPolicy(LocalItemId<RetryPolicyMarker>),
    Let(LocalItemId<LetMarker>),
    /// An out-of-body `implement I for T { … }` block, which opens a scope for
    /// its own generic parameters.
    Impl(LocalItemId<ImplMarker>),
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
    /// The item this scope was opened for, if any. `None` for structural scopes
    /// (Project/Package/Namespace/File) and for expression scopes (`Block`,
    /// `Lambda`, `MatchArm`, …).
    ///
    /// Lives on `Scope` rather than in a parallel vec so it cannot drift out of
    /// step with the scope arena.
    pub owner: Option<ItemScopeOwner>,
    /// Source range of this scope. Used by `scope_at_offset()` to find the
    /// innermost scope containing a cursor position. Structural scopes
    /// (Project, Package, Namespace) use the file's full range.
    pub range: TextRange,
    /// Contiguous range of descendant scope IDs (DFS pre-order).
    /// All scopes in `descendants` are proper descendants of this scope.
    pub descendants: Range<FileScopeId>,
    /// True for the synthetic `ScopeKind::Lambda` scope a tagged template's
    /// body is walked in (BEP-049 `walk_template_lambda_body`). Unlike a real
    /// lambda, this scope has no backing `Expr::Lambda` node and is type-checked
    /// *inline* in its enclosing function/lambda — it records no bindings of its
    /// own — so it must not be treated as an inference owner (see
    /// `inference_owner_scope`). Always `false` for every other scope.
    pub is_template_body: bool,
}
