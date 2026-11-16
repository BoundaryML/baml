use baml_base::Name;
use baml_compiler2_ast::ast;
use text_size::TextRange;

use crate::{
    ids::{ClassMarker, FunctionMarker, LocalItemId},
    item_tree::GenericParam,
};

/// An interface field stored in the `ItemTree`.
///
/// An interface is a contract, not a concrete data type: nothing is ever
/// parsed or rendered *as* one, so its fields carry no attributes (any
/// written there is rejected at lowering).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceField {
    pub name: Name,
    /// Always present — see [`ast::FieldDef::type_expr`].
    pub type_expr: ast::TypeExpr,
    /// Joined `///` doc-comment lines preceding this declaration.
    pub docstring: Option<String>,
}

/// An interface (BEP-044) stored in the `ItemTree`.
///
/// ALL methods - default (with a body) and required (without) - are full
/// `FunctionMarker` entries in `methods`, the same item kind classes use;
/// a required method is simply a `Function` whose `body` is `None`
/// (rust-analyzer's shape: the has-body distinction matters only to body
/// lowering, never to signatures or resolution).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Interface {
    pub name: Name,
    /// Generic type parameters declared on the interface, each with its bounds.
    pub generic_params: Vec<GenericParam>,
    /// Required interfaces from `requires I1, I2, …`.
    pub requires: Vec<ast::TypeExpr>,
    /// Field signatures declared on the interface. Interface fields cannot
    /// have default values.
    pub fields: Vec<InterfaceField>,
    /// Associated type declarations on the interface (BEP-057).
    pub associated_types: Vec<ast::AssociatedTypeDef>,
    /// Every method, default and required alike (required = `body: None`),
    /// in declaration-list order (defaults first, then required).
    pub methods: Vec<LocalItemId<FunctionMarker>>,
    pub docstring: Option<String>,
    pub span: TextRange,
}

/// An `implements` block stored in the `ItemTree`, keyed by a stable
/// `LocalItemId<ImplMarker>`. Both spellings arrive in one shape: the AST
/// gives an in-body `implements I { … }` the class's generics and the class as
/// its for-target (`TYPE_SYSTEM.md`, "Interface Implementations"), so there is
/// one path from here on. `origin` records how it was written, for the rules
/// that read provenance — an interface with fields is implemented in-body —
/// and must NOT influence resolution, dispatch, or coherence. The targets are
/// kept as raw `TypeExpr`s here; resolution happens lazily in the `impl_data`
/// query so HIR construction stays independent of name resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImplBlock {
    pub for_target: ast::TypeExpr,
    pub generics: Vec<GenericParam>,
    pub origin: ast::ImplOrigin,
    /// The class whose body an in-body block was written in: its identity,
    /// recorded as the builder walks the items (a class's in-body blocks
    /// directly follow it), so attachment never goes through the name.
    /// `None` for an out-of-body block.
    pub owner: Option<LocalItemId<ClassMarker>>,
    pub interface_target: ast::TypeExpr,
    pub field_links: Vec<InterfaceFieldLink>,
    pub associated_type_bindings: Vec<ast::AssociatedTypeBindingDef>,
    pub methods: Vec<LocalItemId<FunctionMarker>>,
    pub span: TextRange,
    /// Leading `///` docstring; an in-body block carries none.
    pub docstring: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceFieldLink {
    pub interface_field: Name,
    pub class_field: Name,
    pub span: TextRange,
    pub interface_field_span: TextRange,
    pub class_field_span: TextRange,
}

impl InterfaceFieldLink {
    pub(crate) fn from_ast(link: &ast::InterfaceFieldLinkDef) -> Self {
        Self {
            interface_field: link.interface_field.clone(),
            class_field: link.class_field.clone(),
            span: link.span,
            interface_field_span: link.interface_field_span,
            class_field_span: link.class_field_span,
        }
    }
}
