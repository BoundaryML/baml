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

/// One dependency edge: the name `root` is spelled by in the depending
/// package, and the package it reaches.
///
/// Names live on edges, never on packages (rust-analyzer's `Dependency {
/// crate_id, name }`): two packages may reach one root under different names,
/// and a root needs no name of its own to be depended on. A package's
/// [`SourceRoot::self_name`] is display metadata and the default edge name a
/// manifest offers; it is never an identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Dependency {
    /// The spelling `root`'s items are qualified by inside the depending
    /// package (`baml` in `baml.Array`).
    pub name: Name,
    pub root: SourceRoot,
}

/// Input: one source root — a directory subtree holding the files of exactly
/// one package. The root IS the package: package identity is this input's
/// id, and every package-level query is keyed on it.
///
/// Every file under the root belongs to the package, and namespaces are
/// derived from `ns_*` path segments relative to `path`. Per-root inputs
/// (rather than one big table payload) keep invalidation scoped: adding or
/// removing a file in one root bumps only that root's `files`, so another
/// package's file-set-derived queries are untouched.
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

    pub kind: SourceRootKind,

    /// The package's own name: `[package].name` from its manifest, or the
    /// language-fixed name of a stdlib package. `None` for an unnamed package
    /// (a project with no manifest, a runtime-compiled package).
    ///
    /// Display metadata and the default name a dependent spells this package
    /// by; never compared for identity — the root id is the identity, and the
    /// name a package is reached by lives on the depending package's edge
    /// ([`Dependency::name`]).
    pub self_name: Option<Name>,

    /// Files in this root, in insertion order.
    #[returns(ref)]
    pub files: Vec<SourceFile>,

    /// The package's serialized compiler interface (`borsh(PackageInterface)`,
    /// versioned as a `baml_artifact`), when the package is served from one
    /// instead of from source: a runtime mount, or a precompiled stdlib
    /// package in a runtime compile. When present it is the semantic
    /// authority for the package; any `files` are link-only stubs.
    #[returns(ref)]
    pub interface: Option<Vec<u8>>,

    /// The packages this package may reach, each under the name it spells
    /// them by. The whole dependency graph is the union of these per-root
    /// edge lists; a root reaches nothing it does not list.
    #[returns(ref)]
    pub dependencies: Vec<Dependency>,
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

/// Session-local order: by salsa id, i.e. creation order.
///
/// The type algebra requires `Ord` on a nominal head (`baml_type::Head`) to
/// sort union members into canonical form and to pick a μ-binder's rendering
/// representative, and a package IS a root, so the root must be orderable.
/// This order is deterministic for every artifact-producing path (stdlib
/// roots in manifest order, then the workspace root, then runtime mounts in
/// alias order) and is never serialized or rendered: every artifact boundary
/// spells a root by name and orders by that spelling.
impl PartialOrd for SourceRoot {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SourceRoot {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        salsa::plumbing::AsId::as_id(self).cmp(&salsa::plumbing::AsId::as_id(other))
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
