//! `TypeScriptClass` — shared TypeScript class definition.
//!
//! Covers user-code classes, stdlib classes (`baml.http.Response`, …),
//! and `$stream` companion classes. Emits `export class` (with fields and a
//! field-object constructor); the five runtime-owned stdlib types
//! (media + stream) instead re-export from the configured runtime package.

use baml_codegen_types::{Name, Ty};

use crate::emit::method::TypeScriptMethodBinding;

pub(crate) struct TypeScriptClass {
    /// TS identifier (bare name). The `$stream` suffix is preserved verbatim
    /// (e.g. `Resume$stream`) — `$` is a valid TS identifier char, so the
    /// stream companion is emitted beside its base type in the same module.
    pub(crate) name: String,
    /// Source pool key. Retained for typemap registration and to detect
    /// the five runtime-owned stdlib types (media + stream).
    pub(crate) source: Name,
    /// `TypeVar` names declared on this class.
    pub(crate) generic_params: Vec<String>,
    /// Joined `///` doc-comment lines from the BAML class declaration.
    pub(crate) docstring: Option<String>,
    /// Class fields, in IR declaration order.
    pub(crate) properties: Vec<TypeScriptClassProperty>,
    /// Static method bindings, fanned out into one entry per emitted line.
    pub(crate) static_methods: Vec<TypeScriptMethodBinding>,
    /// Instance method bindings, same shape as `static_methods`.
    pub(crate) instance_methods: Vec<TypeScriptMethodBinding>,
}

pub(crate) struct TypeScriptClassProperty {
    pub(crate) name: String,
    pub(crate) ty: Ty,
    /// Joined `///` doc-comment lines preceding the field.
    pub(crate) docstring: Option<String>,
}
