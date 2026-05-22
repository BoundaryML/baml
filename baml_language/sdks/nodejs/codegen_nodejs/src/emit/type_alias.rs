//! `NodeTypeAlias` — TypeScript type alias.

use baml_codegen_types::{Name, Ty};

pub(crate) struct NodeTypeAlias {
    pub(crate) name: String,
    pub(crate) source: Name,
    /// RHS type — fed to `translate_ty` at render time.
    pub(crate) resolves_to: Ty,
    /// Recursive aliases work natively in TS (the alias name is in scope
    /// inside its own RHS), so the field is kept for parity with
    /// `codegen_python` but the Phase 4 emitter doesn't branch on it.
    #[allow(dead_code)]
    pub(crate) recursive: bool,
}
