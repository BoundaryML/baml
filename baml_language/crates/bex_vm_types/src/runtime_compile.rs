//! Compiler-neutral values crossing the runtime compilation seam.
//!
//! These types deliberately live below both `bex_engine` and `baml_project`:
//! the engine owns only an injected compiler trait object, while the concrete
//! compiler implementation is assembled in `bex_project`.

use std::sync::{
    Arc, Weak,
    atomic::{AtomicBool, Ordering},
};

use baml_type::{Interface, Name, Ty, TypeName};
use indexmap::IndexMap;

/// Compiler-neutral structural projection of one runtime class declaration
/// of a package that had no compile: its row, keyed by the bare item name it
/// is exported under. Field types are spelled from the package's viewpoint
/// (see [`RuntimeProjectedSurface`]).
#[derive(Clone, Debug)]
pub struct RuntimeMountedClass {
    pub name: Name,
    pub docstring: Option<String>,
    pub fields: Vec<(Name, Ty<TypeName>, RuntimeMountedFieldAttrs)>,
}

#[derive(Clone, Debug)]
pub struct RuntimeMountedEnum {
    pub name: Name,
    pub docstring: Option<String>,
    pub variants: Vec<(Name, RuntimeMountedVariantAttrs)>,
}

#[derive(Clone, Debug, Default)]
pub struct RuntimeMountedFieldAttrs {
    pub alias: Option<String>,
    pub description: Option<String>,
    pub docstring: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct RuntimeMountedVariantAttrs {
    pub docstring: Option<String>,
}

/// A type alias a package that had no compile declares: a `with_types` name
/// given a type that is not a bare nominal.
#[derive(Clone, Debug)]
pub struct RuntimeMountedAlias {
    pub name: crate::types::LocalName,
    pub ty: Ty<TypeName>,
}

/// A declaration of another package that a package exports under a name of
/// its own — Rust's `pub use other::Decl as Name`. `of` spells the defining
/// declaration through the package's edges.
#[derive(Clone, Debug)]
pub struct RuntimeReExport {
    pub name: crate::types::LocalName,
    pub of: TypeName,
    pub kind: RuntimeReExportKind,
}

/// What kind of declaration a re-export names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeReExportKind {
    Class,
    Enum,
    Interface,
    TypeAlias,
}

/// One impl rule of a package that had no compile (a witness a minted class
/// carries), spelled from the package's viewpoint: the interface with its
/// arguments and associated bindings, the implementing type, and the
/// `(field, class field)` links the implementation is read through.
#[derive(Clone, Debug)]
pub struct RuntimeMountedImpl {
    pub interface: Interface<TypeName>,
    pub for_ty: Ty<TypeName>,
    pub field_links: Vec<(Name, Name)>,
}

/// The exports of a mount that had no compile — a `with_types` view, or an
/// anonymous declaration a view exports, a root of one row — projected from
/// what it holds.
/// Every type is spelled from the package's own viewpoint: its own
/// declarations `Local`, the prelude under its fixed names, and any other
/// package's declaration through the edge of [`RuntimePackageMount::edges`]
/// that reaches its owner. Every export of a
/// [`ReExported`](crate::types::EdgeKind::ReExported) edge's target is an
/// export of the package too; the compiler adds those from the target's
/// interface.
#[derive(Clone, Debug, Default)]
pub struct RuntimeProjectedSurface {
    pub classes: Vec<RuntimeMountedClass>,
    pub enums: Vec<RuntimeMountedEnum>,
    pub aliases: Vec<RuntimeMountedAlias>,
    pub reexports: Vec<RuntimeReExport>,
    pub impls: Vec<RuntimeMountedImpl>,
}

/// How a mounted package's exports reach the compiler.
#[derive(Clone, Debug)]
pub enum RuntimeMountSurface {
    /// The versioned `PackageInterface` artifact the package's compile
    /// exported.
    Compiled(Vec<u8>),
    /// The package had no compile; its exports are projected from its
    /// tables.
    Projected(RuntimeProjectedSurface),
}

/// The identity of a mounted object across the compile seam — a package, or
/// a declaration belonging to no package that a view exports: an opaque
/// token minted from the object's address, valid for the request that
/// carries it (the request pins its packages). Two aliases naming one
/// package object carry one identity, and two views exporting one anonymous
/// declaration name one identity, so the compile world mounts one root for
/// each, never two look-alikes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RuntimePackageIdentity(usize);

impl RuntimePackageIdentity {
    /// The identity of the package or declaration object at `ptr`.
    pub fn of(ptr: crate::HeapPtr) -> Self {
        Self(ptr.as_ptr() as usize)
    }

    /// A distinct identity for tests that hold no runtime object.
    pub fn synthetic(token: usize) -> Self {
        Self(token)
    }
}

/// One edge of a mount, by the identity of the package or anonymous
/// declaration it reaches. The prelude never crosses the seam: every package
/// reaches it under its fixed names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RuntimeMountEdge {
    pub target: RuntimePackageIdentity,
    pub kind: crate::types::EdgeKind,
}

/// One mount of a compile request, by identity — a package, or an anonymous
/// declaration a view exports (a root of one row): its edges to the other
/// mounts of the request, and how its exports reach the compiler.
#[derive(Clone, Debug)]
pub struct RuntimePackageMount {
    pub edges: IndexMap<Name, RuntimeMountEdge>,
    pub surface: RuntimeMountSurface,
}

/// The kind of a name retained in a Session's compile-time scope.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionVisibleKind {
    Declaration,
    Let,
    TypeBinding { type_value: String },
}

/// One source-visible name and the hygienic name used in replayed source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionVisibleSymbol {
    pub internal: String,
    pub kind: SessionVisibleKind,
}

/// The `eval<T>` contract, spelled for the compiler's name-headed world.
///
/// Converted from the head-typed contract **at request construction, under
/// the heap permit**: the compile task runs after the VM releases its permit,
/// so a head crossing into it could go stale the moment a collection moves the
/// declaration it points at. Nothing heap-shaped may live in the request.
#[derive(Clone, Debug)]
pub enum SessionContract {
    /// A contract every head of which has a declared name — checkable by the
    /// compiler. `unknown` for an uncontracted eval.
    Checkable(baml_type::RuntimeTy),
    /// The contract names a runtime-created declaration, which has no name
    /// the compiler can check a submission against. Carried as a fact so the
    /// compiler reports the unstateable contract rather than comparing a
    /// stand-in.
    NamesRuntimeDeclaration,
}

/// Session-specific inputs copied out of the heap before the compiler yield.
#[derive(Clone, Debug)]
pub struct RuntimeSessionCompileRequest {
    /// Stable virtual file name for the new submission.
    pub submission_name: String,
    /// The user's source, before hygienic session lowering.
    pub source: String,
    /// Successfully committed prior submissions, already lowered and named.
    pub history: IndexMap<String, String>,
    /// The newest source-visible binding for every flat-scope name.
    pub visible: IndexMap<String, SessionVisibleSymbol>,
    /// Runtime contract supplied by `eval<T>` (unknown for uncontracted eval).
    pub expected: SessionContract,
    /// Keeps the one-eval permit live across compile and execution.
    pub lease: SessionEvalLease,
}

/// RAII permit for S-9. Every cancellation/error path releases the busy bit
/// simply by dropping the last clone; successful evaluation releases it
/// explicitly after the final continuation.
#[derive(Clone)]
pub struct SessionEvalLease(Arc<SessionEvalLeaseInner>);

#[derive(Clone)]
pub(crate) struct WeakSessionEvalLease(Weak<SessionEvalLeaseInner>);

#[derive(Debug)]
struct SessionEvalLeaseInner {
    busy: Arc<AtomicBool>,
}

impl SessionEvalLease {
    pub fn acquire(busy: Arc<AtomicBool>) -> Option<Self> {
        busy.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| Self(Arc::new(SessionEvalLeaseInner { busy })))
    }

    pub fn release(&self) {
        self.0.busy.store(false, Ordering::Release);
    }

    pub(crate) fn downgrade(&self) -> WeakSessionEvalLease {
        WeakSessionEvalLease(Arc::downgrade(&self.0))
    }
}

impl WeakSessionEvalLease {
    pub(crate) fn release(&self) {
        if let Some(lease) = self.0.upgrade() {
            lease.busy.store(false, Ordering::Release);
        }
    }
}

impl std::fmt::Debug for SessionEvalLease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionEvalLease").finish_non_exhaustive()
    }
}

impl Drop for SessionEvalLeaseInner {
    fn drop(&mut self) {
        self.busy.store(false, Ordering::Release);
    }
}

/// Which runtime compilation door created a request.
#[derive(Clone, Debug, Default)]
pub enum RuntimeCompileMode {
    #[default]
    Package,
    Session(Box<RuntimeSessionCompileRequest>),
}

/// One isolated `reflect.Package.compile` request.
#[derive(Clone, Debug, Default)]
pub struct RuntimeCompileRequest {
    /// Project-root-relative submitted paths and their source text.
    pub files: IndexMap<String, String>,
    /// Every package the compile can reach, by identity: the packages
    /// mounted under an alias and, transitively, every package their edges
    /// reach — a consumer names a re-exported declaration by the package it
    /// is defined in.
    pub packages: IndexMap<RuntimePackageIdentity, RuntimePackageMount>,
    /// The compile's own edges: each alias to the package it mounts.
    pub aliases: IndexMap<String, RuntimePackageIdentity>,
    pub mode: RuntimeCompileMode,
}

/// Severity retained from the compiler diagnostic stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeDiagnosticSeverity {
    Error,
    Warning,
    Info,
}

/// Compiler phase retained from the structured compiler diagnostic stream.
///
/// Runtime-generated diagnostics (linking, mounting, session contracts, and
/// similar host-side failures) have no compiler phase and therefore carry no
/// [`RuntimeDiagnosticDetails`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeDiagnosticPhase {
    Parse,
    Hir,
    Validation,
    Type,
}

/// Semantic category attached to a byte range inside diagnostic text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeDiagnosticHighlightKind {
    IdentifierType,
    IdentifierFunction,
    IdentifierField,
    IdentifierVariable,
    IdentifierEnumVariant,
    IdentifierAttribute,
    TypeExpression,
    Code,
}

/// A byte range inside a diagnostic message or annotation label.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeDiagnosticHighlight {
    pub start: u32,
    pub end: u32,
    pub kind: RuntimeDiagnosticHighlightKind,
}

/// A byte range in one of the paths submitted to the compile call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeSourceSpan {
    pub file: String,
    pub start: usize,
    pub end: usize,
}

/// One primary or secondary source annotation on a compiler diagnostic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeDiagnosticAnnotation {
    pub span: RuntimeSourceSpan,
    pub message: Option<String>,
    pub message_highlights: Vec<RuntimeDiagnosticHighlight>,
    pub is_primary: bool,
}

/// One related source location attached to a compiler diagnostic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeDiagnosticRelatedInfo {
    pub span: RuntimeSourceSpan,
    pub message: String,
    pub message_highlights: Vec<RuntimeDiagnosticHighlight>,
    pub file_path: Option<String>,
}

/// Compiler-only diagnostic detail retained while the transient compiler DB
/// is alive. The legacy flattened `message` and primary `span` remain directly
/// on [`RuntimeCompileDiagnostic`] for compatibility with existing consumers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeDiagnosticDetails {
    pub headline: String,
    pub primary_label: Option<String>,
    pub phase: RuntimeDiagnosticPhase,
    pub message_highlights: Vec<RuntimeDiagnosticHighlight>,
    pub annotations: Vec<RuntimeDiagnosticAnnotation>,
    pub related_info: Vec<RuntimeDiagnosticRelatedInfo>,
}

/// Stable diagnostic data safe to retain after the transient compiler DB drops.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeCompileDiagnostic {
    pub code: String,
    pub message: String,
    pub severity: RuntimeDiagnosticSeverity,
    pub span: Option<RuntimeSourceSpan>,
    pub details: Option<Box<RuntimeDiagnosticDetails>>,
}
