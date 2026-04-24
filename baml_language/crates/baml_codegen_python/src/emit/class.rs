//! `PyClass` — Python class definition.
//!
//! Covers user-code classes, stdlib classes (`baml.http.Response`, …),
//! and `$stream` companion classes. All three render as a
//! `pydantic.BaseModel` subclass with typed fields; they differ only in
//! leaf routing.

use baml_codegen_types::{Name, Ty};

/// Python class definition. G4 populates `properties`; §7 handle-backed
/// classes are deferred — every `PyClass` renders as vanilla Pydantic.
pub(crate) struct PyClass {
    /// Python identifier (bare name). `$stream` suffix is stripped —
    /// it influenced routing, not the class name.
    pub(crate) py_name: String,
    /// Source pool key, retained for debug / routing.
    #[allow(dead_code)]
    pub(crate) source: Name,
    /// Class fields, in IR declaration order. Field names are emitted
    /// verbatim per 09b §5 ("agent-friendly" naming).
    pub(crate) properties: Vec<PyClassProperty>,
    // deferred to G4+: docstring: Option<String>,
    // deferred: handle_backed: bool,
    // deferred to G5: methods: Vec<PyMethod>,
}

pub(crate) struct PyClassProperty {
    pub(crate) name: String,
    pub(crate) ty: Ty,
}
