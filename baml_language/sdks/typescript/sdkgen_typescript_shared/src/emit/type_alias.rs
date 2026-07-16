//! `TypeScriptTypeAlias` — shared TypeScript type alias.
//!
//! Emits `export type Foo = <RHS>`. `recursive` is consumed by
//! `group_and_sort` (recursive aliases hoist to the front of the leaf).

use baml_codegen_types::{Name, Ty};

pub(crate) struct TypeScriptTypeAlias {
    pub(crate) name: String,
    pub(crate) source: Name,
    /// The `Ty` that this alias resolves to; fed to `translate_ty` in
    /// Phase 3.
    pub(crate) resolves_to: Ty,
    /// If true, hoist to the front of the leaf so a self-reference
    /// resolves after the alias declaration.
    pub(crate) recursive: bool,
}
