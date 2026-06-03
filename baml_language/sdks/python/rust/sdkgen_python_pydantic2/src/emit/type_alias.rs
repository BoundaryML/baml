//! `PyTypeAlias` — Python type alias.

use baml_codegen_types::{Name, Ty};

/// Python type alias.
///
/// - Non-recursive: renders as `Foo: typing.TypeAlias = <RHS>`.
/// - Recursive: renders as
///   `Foo = typing_extensions.TypeAliasType("Foo", <RHS>)` with inner
///   self-references quoted as `"Foo"` forward-refs (18c).
///   Pydantic recognizes `TypeAliasType` natively and routes recursive
///   self-references through its JSON-schema `$ref` machinery, so a
///   `BaseModel` field annotated with the alias no longer recurses
///   forever during schema build.
pub(crate) struct PyTypeAlias {
    pub(crate) py_name: String,
    pub(crate) source: Name,
    /// The `Ty` that this alias resolves to; fed verbatim to the G3
    /// translator at render time.
    pub(crate) resolves_to: Ty,
    /// If true, render via `typing_extensions.TypeAliasType` so
    /// recursive self-references resolve through Pydantic's
    /// definitions / `$ref` machinery instead of inlining.
    pub(crate) recursive: bool,
}
