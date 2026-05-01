//! `PyTypeAlias` — Python type alias.

use baml_codegen_types::{Name, Ty};

/// Python type alias. Renders as `Foo: typing.TypeAlias = <RHS>`, or
/// `Foo: typing.TypeAlias = '<RHS>'` when `recursive == true` (whole-RHS
/// string quoting per 09b §4.3 / G4 §8).
pub(crate) struct PyTypeAlias {
    pub(crate) py_name: String,
    #[allow(dead_code)]
    pub(crate) source: Name,
    /// The `Ty` that this alias resolves to; fed verbatim to the G3
    /// translator at render time.
    pub(crate) resolves_to: Ty,
    /// If true, the entire RHS is Python-string-quoted so Pydantic can
    /// resolve the alias lazily at `model_rebuild` time.
    pub(crate) recursive: bool,
}
