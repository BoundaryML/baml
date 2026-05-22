//! `NodeClass` — TypeScript class definition.
//!
//! Phase 2 only carries the minimum needed to render a placeholder
//! (`name`, `source` for typemap registration). Properties are kept so
//! Phase 4 can fill in the real class body without re-touching the
//! emitter scaffolding.

use baml_codegen_types::{Name, Ty};

use crate::emit::method::NodeMethodBinding;

pub(crate) struct NodeClass {
    /// TS identifier (bare name). `$stream` suffix is stripped — it
    /// influenced routing, not the class name.
    pub(crate) name: String,
    /// Source pool key, retained for typemap registration.
    pub(crate) source: Name,
    /// `TypeVar` names declared on this class.
    pub(crate) generic_params: Vec<String>,
    /// Joined `///` doc-comment lines from the BAML class declaration.
    /// Phase 2 keeps but doesn't render.
    #[allow(dead_code)]
    pub(crate) docstring: Option<String>,
    /// Class fields in IR declaration order. Phase 2 keeps but doesn't render.
    #[allow(dead_code)]
    pub(crate) properties: Vec<NodeClassProperty>,
    /// Static method bindings — Phase 4 surfaces them inside the class body.
    #[allow(dead_code)]
    pub(crate) static_methods: Vec<NodeMethodBinding>,
    /// Instance method bindings — Phase 4 surfaces them inside the class body.
    #[allow(dead_code)]
    pub(crate) instance_methods: Vec<NodeMethodBinding>,
}

#[allow(dead_code)]
pub(crate) struct NodeClassProperty {
    pub(crate) name: String,
    pub(crate) ty: Ty,
    pub(crate) docstring: Option<String>,
}
