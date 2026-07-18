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
    /// Joined `///` doc-comment lines from the BAML enum declaration.
    /// Combined with `PyEnumVariant.docstring` from each entry in
    /// `variants` to produce the `"""…"""` Python enum docstring;
    /// `Enum.__doc__` carries the summary plus a `Members:` section
    /// (see `crate::utils::format_class_docstring`).
    pub(crate) docstring: Option<String>,
}

pub(crate) struct PyEnumVariant {
    /// LHS identifier.
    pub(crate) ident: String,
    /// RHS string literal (IR's `EnumVariant.value`, verbatim).
    pub(crate) value: String,
    /// `Some(raw)` when `ident` was escaped off a hard keyword — the raw BAML
    /// variant name, which stays the wire spelling. Drives the enum's
    /// `__baml_wire_values__` provenance marker so the bridge encoder
    /// can recover the wire name without shape-guessing. `None` for the common
    /// case (member renders byte-identically to today), mirroring
    /// `PyClassProperty::alias`.
    pub(crate) wire_name: Option<String>,
    /// Joined `///` doc-comment lines preceding the variant. Folded
    /// into the parent `PyEnum`'s `"""…"""` docstring under a
    /// `Members:` section — never rendered as an inline `# …` comment
    /// or PEP-257 attribute docstring next to the variant. The
    /// visibility rule for the section lives in
    /// `crate::utils::format_class_docstring`.
    pub(crate) docstring: Option<String>,
}
