//! Type rendering as a parameterized service.
//!
//! Two absolute presets, both context-free — the same `Ty` renders
//! identically no matter which file or package asks. Scope-relative
//! rendering (eliding names visible from a given scope, for hover text) is
//! deliberately a *separate* function that will take the scope explicitly
//! when the editor layer migrates; making "no context" the default is what
//! keeps file-relative spellings like `root.errors.Io` out of artifacts.

use baml_type::Ty;

/// A named rendering policy for types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TyDisplayFormat {
    /// Fully qualified and round-trippable: `user.errors.X`, `baml.errors.Io`.
    /// The policy for artifacts, ids, and anything diffed or parsed.
    #[default]
    Canonical,
    /// The human spelling: the implicit `user` package elided
    /// (`errors.X`), dependency names kept (`baml.errors.Io`).
    UserFacing,
}

impl TyDisplayFormat {
    pub fn render(self, ty: &Ty) -> String {
        match self {
            Self::Canonical => ty.render_canonical(),
            Self::UserFacing => ty.render_user_facing(),
        }
    }
}
