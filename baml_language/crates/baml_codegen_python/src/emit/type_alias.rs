//! `PyTypeAlias` — stub for a Python type alias.

use baml_codegen_types::Name;

/// Stub for a Python type alias. Renders as
/// `Foo: typing.TypeAlias = typing.Any` — `typing.Any` is the lowest-
/// information placeholder that still parses and type-checks.
pub(crate) struct PyTypeAlias {
    pub(crate) py_name: String,
    #[allow(dead_code)]
    pub(crate) source: Name,
    // deferred to G3+G4: resolves_to: PyTypeExpr,
    // deferred to G4: recursive: bool,
}
