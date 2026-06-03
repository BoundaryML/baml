//! `PyClass` — Python class definition.
//!
//! Covers user-code classes, stdlib classes (`baml.http.Response`, …),
//! and `$stream` companion classes. All three render as a
//! `pydantic.BaseModel` subclass with typed fields; they differ only in
//! leaf routing.

use baml_codegen_types::{Name, Ty};

use crate::emit::method::PyMethodBinding;

/// Python class definition. G4 populates `properties`; 12b populates
/// `static_methods` and `instance_methods`; §7 handle-backed
/// classes are deferred — every `PyClass` renders as vanilla Pydantic.
pub(crate) struct PyClass {
    /// Python identifier (bare name). `$stream` suffix is stripped —
    /// it influenced routing, not the class name.
    pub(crate) py_name: String,
    /// Source pool key, retained for debug / routing.
    #[allow(dead_code)]
    pub(crate) source: Name,
    /// `TypeVar` names declared on this class. Empty for non-generic
    /// classes. When populated, the class line gets a
    /// `typing.Generic[T, …]` second base and the leaf-level `TypeVar`
    /// declarations include each name.
    pub(crate) generic_params: Vec<String>,
    /// Joined `///` doc-comment lines from the BAML class declaration.
    /// Combined with `PyClassProperty.docstring` from each entry in
    /// `properties` to produce the `"""…"""` Python class docstring;
    /// `Class.__doc__` carries the summary plus an `Attributes:`
    /// section (see `crate::utils::format_class_docstring`).
    pub(crate) docstring: Option<String>,
    /// Class fields, in IR declaration order. Field names are emitted
    /// verbatim per 09b §5 ("agent-friendly" naming).
    pub(crate) properties: Vec<PyClassProperty>,
    /// Static method bindings, fanned out into one entry per emitted
    /// line (sync, async, companion sync, companion async, …) in the
    /// final render order. Pre-sorted by the expander so the renderer
    /// is a straight walk.
    pub(crate) static_methods: Vec<PyMethodBinding>,
    /// Instance method bindings, same shape as `static_methods`.
    pub(crate) instance_methods: Vec<PyMethodBinding>,
}

pub(crate) struct PyClassProperty {
    pub(crate) name: String,
    pub(crate) ty: Ty,
    /// Joined `///` doc-comment lines preceding the field. Folded into
    /// the parent `PyClass`'s `"""…"""` docstring under an
    /// `Attributes:` section — never rendered as an inline `# …`
    /// comment next to the field declaration. The visibility rule
    /// for the section lives in `crate::utils::format_class_docstring`.
    pub(crate) docstring: Option<String>,
}
