//! `PyClass` — Python class definition.
//!
//! Covers user-code classes, stdlib classes (`baml.http.Response`, …),
//! and `$stream` companion classes. All three render as a
//! `pydantic.BaseModel` subclass with typed fields; they differ only in
//! leaf routing.

use std::collections::BTreeMap;

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
    /// Raw BAML TypeVar names matching `generic_params`.
    pub(crate) wire_generic_params: Vec<String>,
    /// Raw TypeVar spelling -> projected Python spelling for annotations.
    pub(crate) type_var_names: BTreeMap<String, String>,
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
    /// Projected Python attribute name.
    pub(crate) name: String,
    /// Raw BAML field name used for validation and wire serialization.
    pub(crate) wire_name: String,
    pub(crate) ty: Ty,
    /// Whether the field's top-level BAML type admits `null`. This is kept
    /// separate from its rendered annotation so generator policy never relies
    /// on parsing Python type strings.
    pub(crate) nullable: bool,
    /// Joined `///` doc-comment lines preceding the field. Folded into
    /// the parent `PyClass`'s `"""…"""` docstring under an
    /// `Attributes:` section — never rendered as an inline `# …`
    /// comment next to the field declaration. The visibility rule
    /// for the section lives in `crate::utils::format_class_docstring`.
    pub(crate) docstring: Option<String>,
}
