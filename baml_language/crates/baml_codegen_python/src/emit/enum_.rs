//! `PyEnum` — stub for a Python enum.

use baml_codegen_types::Name;

/// Stub for a Python enum. Renders as `class Foo(str, enum.Enum): pass`.
/// The `str` mixin stays even in G2 because it's cheap and getting the
/// base-class tuple wrong here would mask bugs later.
pub(crate) struct PyEnum {
    pub(crate) py_name: String,
    #[allow(dead_code)]
    pub(crate) source: Name,
    // deferred to G4: variants: Vec<PyEnumVariant>,
    // deferred to G4: docstring: Option<String>,
}
