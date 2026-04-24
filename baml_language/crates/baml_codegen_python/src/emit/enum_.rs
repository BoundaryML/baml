//! `PyEnum` — Python enum definition.

use baml_codegen_types::Name;

/// Python enum definition. Renders as `class Foo(str, enum.Enum): …`
/// with `<VARIANT> = "<value>"` lines in IR order.
pub(crate) struct PyEnum {
    pub(crate) py_name: String,
    #[allow(dead_code)]
    pub(crate) source: Name,
    /// Enum variants in IR declaration order.
    pub(crate) variants: Vec<PyEnumVariant>,
    // deferred to G4+: docstring: Option<String>,
}

pub(crate) struct PyEnumVariant {
    /// LHS identifier.
    pub(crate) ident: String,
    /// RHS string literal (IR's `EnumVariant.value`, verbatim).
    pub(crate) value: String,
}
