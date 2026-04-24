//! `PyClass` — stub for a Python class definition.
//!
//! Covers user-code classes, stdlib classes (`baml.http.Response`, …),
//! and `$stream` companion classes. All three render as `class X: pass`
//! in G2; they differ only in leaf routing.

use baml_codegen_types::Name;

/// Stub for a Python class definition.
pub(crate) struct PyClass {
    /// Python identifier (bare name). `$stream` suffix is stripped —
    /// it influenced routing, not the class name.
    pub(crate) py_name: String,
    /// Source pool key, retained for debug / routing.
    #[allow(dead_code)]
    pub(crate) source: Name,
    // deferred to G4: properties: Vec<PyClassProperty>,
    // deferred to G4: docstring: Option<String>,
    // deferred to G4: handle_backed: bool,
    // deferred to G5: methods: Vec<PyMethod>,
}
