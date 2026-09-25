//! What a runtime compile produces and what a `reflect.CompileArtifact`
//! holds between `_compile` and `_finish`.

use baml_linker_types::{EmittedPackage, ItemPath, SessionInitializer};
use bex_heap::Handle;
use bex_vm_types::{RuntimeCompileDiagnostic, SessionEvalLease, SessionVisibleSymbol};
use indexmap::IndexMap;

/// Successful compiler output retained by the runtime: one package's
/// output in the unit convention, loaded by the graft against the packages
/// it was compiled against.
#[derive(Debug)]
pub struct RuntimeCompileArtifact {
    /// The package (or submission) as emitted: builtin and dependency
    /// declarations are imports the graft binds.
    pub emitted: EmittedPackage,
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
    /// The submission's `let` helpers as the emitter recorded them, in
    /// execution order.
    pub initializers: Vec<SessionInitializer>,
}

/// What one emitted initializer commits when it returns successfully.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeSessionStep {
    /// The generated `let` receiving the initializer's result.
    pub global: ItemPath,
    /// An existing session cell to update after this initializer succeeds.
    /// `None` means the generated `let` itself receives the value.
    pub commit_global: Option<ItemPath>,
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

/// A compile's output together with the dependency packages it was compiled
/// against, each pinned by a rooted [`Handle`] under the alias the request
/// spelled it by. `_finish` binds the package's dependencies from these pins
/// — never from its own argument, which it only checks against them — so the
/// declarations the output references are the ones the compiler read, and
/// they stay alive until the artifact is consumed or dropped. A session's
/// artifact pins nothing: its dependency table is the session package's.
pub struct PinnedArtifact {
    pub artifact: RuntimeCompileArtifact,
    pub pins: IndexMap<String, Handle>,
}

/// One-shot storage used by the BAML `CompileArtifact` wrapper.
pub type RuntimeCompileArtifactSlot = std::sync::Mutex<Option<PinnedArtifact>>;
