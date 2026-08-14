use baml_base::Name;
use baml_compiler2_ast::ast;
use text_size::TextRange;

use crate::{
    ids::{ClassMarker, FunctionMarker, LocalItemId},
    item_tree::{Attribute, ClassField, GenericParam},
};

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
    pub fields: Vec<ClassField>,
    /// Associated type declarations on the interface (BEP-057).
    pub associated_types: Vec<ast::AssociatedTypeDef>,
    /// Every method, default and required alike (required = `body: None`),
    /// in declaration-list order (defaults first, then required).
    pub methods: Vec<LocalItemId<FunctionMarker>>,
    pub attributes: Vec<Attribute>,
    pub docstring: Option<String>,
    pub span: TextRange,
}

/// What an `implements` block applies to. Unifying the owner with the for-target
/// makes "in-body with an explicit for-target" and "out-of-body without one"
/// both unrepresentable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImplSubject {
    /// `implements I { … }` in a class body (or a simple `implement I for C`
    /// merged onto `C`). The for-type is the class itself; the generics are the
    /// class's. `out_of_body` records the syntactic origin for diagnostics only
    /// — it must NOT influence resolution, dispatch, or coherence.
    InClass {
        class: LocalItemId<ClassMarker>,
        out_of_body: bool,
    },
    /// `implement<…> I for <for_target> { … }`: an explicit for-type plus the
    /// block's own generic parameters.
    Free {
        for_target: ast::TypeExpr,
        generics: Vec<GenericParam>,
    },
}

/// A unified `implements` block (both kinds) stored in the `ItemTree`, keyed by
/// a stable `LocalItemId<ImplMarker>`. The interface target is kept as a raw
/// `TypeExpr` here; resolution to an `InterfaceLoc` + `Ty` happens lazily in the
/// `impl_data` query so HIR construction stays independent of name resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImplBlock {
    pub subject: ImplSubject,
    pub interface_target: ast::TypeExpr,
    pub field_links: Vec<InterfaceFieldLink>,
    pub associated_type_bindings: Vec<ast::AssociatedTypeBindingDef>,
    pub methods: Vec<LocalItemId<FunctionMarker>>,
    pub span: TextRange,
    /// Leading `///` docstring — populated for free `implements … for …`
    /// blocks; in-body `implements I { … }` blocks do not carry one today.
    pub docstring: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImplementsBlock {
    pub target: ast::TypeExpr,
    pub field_links: Vec<InterfaceFieldLink>,
    pub associated_type_bindings: Vec<ast::AssociatedTypeBindingDef>,
    pub is_out_of_body: bool,
    pub span: TextRange,
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
