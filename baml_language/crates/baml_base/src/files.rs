//! File management with Salsa 2022 API.
//!
//! Defines the core structures for accessing file contents and paths, and the
//! source-root model that partitions those files into packages
//! (rust-analyzer's `SourceRoot` shape).

use std::path::PathBuf;

use crate::{FileId, Name};

/// What kind of package a [`SourceRoot`] holds. Drives editability,
/// diagnostics publication, dependency resolution policy, and the position
/// of the root's files in every whole-program index space.
///
/// Variants are declared in table order (see [`SourceRootTable`]):
/// `Stdlib` < `Dependency` < `Workspace` < `Dynamic`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SourceRootKind {
    /// Embedded stdlib source (`baml_builtins2`). Read-only, never published
    /// to editors as diagnostics, and always first in every index space.
    Stdlib,
    /// A read-only source-bearing dependency package resolved at build time
    /// (a `[dependencies]` entry — path, registry, ...).
    Dependency,
    /// A user project: editable, diagnosed, watched, emitted.
    Workspace,
    /// A package loaded at run time (a runtime mount's stub units, an eval
    /// session's submissions). Always last, so runtime-loaded code never
    /// shifts the indices of the statically compiled prefix.
    Dynamic,
}

/// Input: one source root — a directory subtree holding the files of exactly
/// one package.
///
/// The root is the unit of package identity: every file under it belongs to
/// `package`, and namespaces are derived from `ns_*` path segments relative
/// to `path`. Per-root inputs (rather than one big table payload) keep
/// invalidation scoped: adding or removing a file in one root bumps only that
/// root's `files`, so another package's file-set-derived queries are
/// untouched.
// NOT `(debug)`: `SourceRoot::files` and `SourceFile::source_root` point at
// each other, so field-printing Debug impls on both sides would recurse
// until stack overflow the first time anyone logs a file. The root prints
// id-only (impl below); `SourceFile`'s field-level Debug prints the root as
// that id and terminates.
#[salsa::input]
pub struct SourceRoot {
    /// Root directory. A real filesystem path for `Workspace` and on-disk
    /// `Dependency` roots; a virtual `<builtin>/<pkg>` path for `Stdlib` and
    /// embedded dependency roots.
    #[returns(ref)]
    pub path: PathBuf,

    /// The compiler package name every file under this root belongs to.
    pub package: Name,

    pub kind: SourceRootKind,

    /// Files in this root, in insertion order.
    #[returns(ref)]
    pub files: Vec<SourceFile>,
}

impl std::fmt::Debug for SourceRoot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Id-only by design: printing `files` would recurse through
        // `SourceFile::source_root` — see the comment on the struct.
        f.debug_tuple("SourceRoot")
            .field(&salsa::plumbing::AsId::as_id(self))
            .finish()
    }
}

/// Input: the ordered set of source roots in the database.
///
/// Order invariant (enforced by the concrete database): all `Stdlib` roots,
/// then `Dependency`, then `Workspace`, then `Dynamic` roots — i.e. sorted by
/// [`SourceRootKind`]'s declaration order. Whole-program iteration
/// (emit's index spaces, MIR's type tags) follows this order, which is what
/// keeps the stdlib a stable prefix of every index space independent of user
/// code — the precondition for splicing a precompiled stdlib `Program` slice
/// into any project's compile.
#[salsa::input(debug)]
pub struct SourceRootTable {
    #[returns(ref)]
    pub roots: Vec<SourceRoot>,
}

/// Input structure representing a source file in the compilation.
///
/// This is a salsa input, which means it's the primary way to provide
/// source text to the compiler. The struct itself just stores an ID,
/// with the actual data stored in the salsa database.
#[salsa::input(debug)]
pub struct SourceFile {
    /// Source text for the file
    #[returns(ref)]
    pub text: String,

    /// File path (for diagnostics and error reporting)
    pub path: PathBuf,

    /// The FileId associated with this source file.
    ///
    /// Used to create lightweight Span values that can be embedded in tokens.
    /// This allows spans to identify their source file without carrying
    /// the full SourceFile reference (which is a Salsa-tracked entity).
    pub file_id: FileId,

    /// Whether this is compiler-generated source for a `Session.eval`
    /// submission. Session lowering represents persistent bindings as root
    /// lets internally; ordinary BAML source files must reject them.
    pub is_session_submission: bool,

    /// The source root this file belongs to.
    ///
    /// An input *fact* set at creation by the database owner, not a tracked
    /// derivation: reading it records a dependency on this field only, so
    /// adding or removing an unrelated root never invalidates a file's
    /// package identity. A file revived from a remove/re-add cycle must have
    /// this field re-set — the owning root may have changed.
    pub source_root: SourceRoot,
}
