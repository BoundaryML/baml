//! Compiler-neutral values crossing the runtime compilation seam.
//!
//! These types deliberately live below both `bex_engine` and `baml_project`:
//! the engine owns only an injected compiler trait object, while the concrete
//! compiler implementation is assembled in `bex_project`.

use std::sync::{
    Arc, Mutex, Weak,
    atomic::{AtomicBool, Ordering},
};

use baml_type::{Interface, Name, RealizedTy, Ty};
use indexmap::IndexMap;

use crate::CompilationUnit;

/// Compiler-neutral structural projection of one runtime class definition.
///
/// `name` is the declaration's bare item name; the compile world spells it
/// `alias.<name>` under whatever alias the package is mounted as. `tag` is the
/// declaration's live identity, carried only so the compile seam can tell two
/// same-named declarations apart (a fail-closed duplicate check) — it is never
/// a compile-time identity.
#[derive(Clone, Debug)]
pub struct RuntimeMountedClass {
    pub name: Name,
    pub tag: baml_type::typetag::TypeTag,
    pub fields: Vec<(Name, Ty, RuntimeMountedFieldAttrs)>,
}

#[derive(Clone, Debug)]
pub struct RuntimeMountedEnum {
    pub name: Name,
    pub tag: baml_type::typetag::TypeTag,
    pub variants: Vec<Name>,
}

#[derive(Clone, Debug, Default)]
pub struct RuntimeMountedFieldAttrs {
    pub alias: Option<String>,
    pub description: Option<String>,
}

/// One exact type value mounted under a source-visible export name.
///
/// `ty` and every field type in `classes` are already spelled from the
/// consumer compile world's viewpoint: a runtime declaration appears as
/// `alias.<item name>`, a compiled one as its own qualified name.
#[derive(Clone, Debug)]
pub struct RuntimeTypeMount {
    pub export_name: Name,
    pub ty: RealizedTy,
    pub classes: Vec<RuntimeMountedClass>,
    pub enums: Vec<RuntimeMountedEnum>,
    pub witnesses: Vec<(Interface, Vec<(Name, Name)>)>,
}

#[derive(Clone, Debug, Default)]
pub struct RuntimePackageMount {
    /// Versioned `PackageInterface` artifact checked before the mount is used.
    pub interface_blob: Vec<u8>,
    pub types: Vec<RuntimeTypeMount>,
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

/// What one emitted initializer commits when it returns successfully.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeSessionStep {
    /// Fully-qualified generated global receiving the initializer result.
    pub global: String,
    /// Existing Session cell to update after this initializer succeeds. `None`
    /// means the generated global itself receives the value.
    pub commit_global: Option<String>,
    pub kind: RuntimeSessionStepKind,
}

/// The two legal commit shapes of a Session initializer step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeSessionStepKind {
    Expression,
    Binding {
        /// Source-visible binding committed by this step.
        name: String,
        symbol: SessionVisibleSymbol,
        /// Replayed source fragment appended only after this step succeeds.
        replay_source: String,
    },
}

/// Compiler-owned Session metadata retained after the fresh database drops.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeSessionCompileArtifact {
    pub submission_name: String,
    /// Hoisted declarations are committed before the first initializer runs.
    pub declaration_source: String,
    pub declarations: IndexMap<String, SessionVisibleSymbol>,
    pub steps: Vec<RuntimeSessionStep>,
    /// The step whose value is the submission's observable result.
    pub result_step: Option<usize>,
    /// Current-submission initializer helpers in execution order. `helper_slot`
    /// addresses the anonymous helper-slot list retained in the pruned tail.
    pub initializers: Vec<RuntimeSessionInitializer>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeSessionInitializer {
    pub helper_slot: u32,
    pub target_global: String,
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
    /// Source-less dependency package name to enriched `PackageInterface` blob.
    pub packages: IndexMap<String, RuntimePackageMount>,
    pub mode: RuntimeCompileMode,
}

/// Severity retained from the compiler diagnostic stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeDiagnosticSeverity {
    Error,
    Warning,
    Info,
}

/// A byte range in one of the paths submitted to the compile call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeSourceSpan {
    pub file: String,
    pub start: usize,
    pub end: usize,
}

/// Stable diagnostic data safe to retain after the transient compiler DB drops.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeCompileDiagnostic {
    pub code: String,
    pub message: String,
    pub severity: RuntimeDiagnosticSeverity,
    pub span: Option<RuntimeSourceSpan>,
}

/// Successful compiler output retained by the runtime.
#[derive(Debug)]
pub struct RuntimeCompileArtifact {
    /// Relocatable user units. Builtin/dependency definitions remain imports.
    pub units: Vec<CompilationUnit>,
    /// Versioned artifact containing the enriched check surface for mounting
    /// this package in a later compile.
    pub interface_blob: Vec<u8>,
    /// Non-error diagnostics produced by the successful compilation.
    pub diagnostics: Vec<RuntimeCompileDiagnostic>,
    pub kind: ArtifactKind,
}

/// Which runtime compilation door produced an artifact.
#[derive(Debug)]
pub enum ArtifactKind {
    Package,
    Session {
        meta: RuntimeSessionCompileArtifact,
        /// S-9 permit transferred from the request to the successful artifact.
        lease: SessionEvalLease,
    },
}

/// One-shot storage used by the BAML `CompileArtifact` wrapper.
pub type RuntimeCompileArtifactSlot = Mutex<Option<RuntimeCompileArtifact>>;
