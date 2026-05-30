//! `NodeTypeAlias` — TypeScript type alias.
//!
//! Phase 2 renders as a `BAML_PLACEHOLDER`; Phase 4 emits the real
//! `export type Foo = <RHS>`. `recursive` is consumed now by
//! `group_and_sort` (recursive aliases hoist to the front of the leaf).

use baml_codegen_types::{Name, Ty};

#[allow(dead_code)]
pub(crate) struct NodeTypeAlias {
    pub(crate) name: String,
    pub(crate) source: Name,
    /// The `Ty` that this alias resolves to; fed to `translate_ty` in
    /// Phase 3.
    pub(crate) resolves_to: Ty,
    /// If true, hoist to the front of the leaf so a self-reference
    /// resolves after the alias declaration.
    pub(crate) recursive: bool,
}
