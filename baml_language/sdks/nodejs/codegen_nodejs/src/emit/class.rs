//! `NodeClass` — TypeScript class definition.
//!
//! Covers user-code classes, stdlib classes (`baml.http.Response`, …),
//! and `$stream` companion classes. Phase 2 renders every class as a
//! `BAML_PLACEHOLDER` (except the five runtime-owned stdlib types, which
//! re-export from `@boundaryml/baml-core`); Phase 4 emits the real
//! `export class`.

use baml_codegen_types::{Name, Ty};

use crate::emit::method::NodeMethodBinding;

#[allow(dead_code)]
pub(crate) struct NodeClass {
    /// TS identifier (bare name). `$stream` suffix is stripped — it
    /// influenced routing, not the class name.
    pub(crate) name: String,
    /// Source pool key. Retained for typemap registration and to detect
    /// the five runtime-owned stdlib types (media + stream).
    pub(crate) source: Name,
    /// `TypeVar` names declared on this class.
    pub(crate) generic_params: Vec<String>,
    /// Joined `///` doc-comment lines from the BAML class declaration.
    pub(crate) docstring: Option<String>,
    /// Class fields, in IR declaration order.
    pub(crate) properties: Vec<NodeClassProperty>,
    /// Static method bindings, fanned out into one entry per emitted line.
    pub(crate) static_methods: Vec<NodeMethodBinding>,
    /// Instance method bindings, same shape as `static_methods`.
    pub(crate) instance_methods: Vec<NodeMethodBinding>,
}

#[allow(dead_code)]
pub(crate) struct NodeClassProperty {
    pub(crate) name: String,
    pub(crate) ty: Ty,
    /// Joined `///` doc-comment lines preceding the field.
    pub(crate) docstring: Option<String>,
}
