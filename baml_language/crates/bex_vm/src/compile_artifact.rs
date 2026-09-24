//! What a `reflect.CompileArtifact` holds between `_compile` and `_finish`.

use bex_heap::Handle;
use bex_vm_types::RuntimeCompileArtifact;
use indexmap::IndexMap;

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
