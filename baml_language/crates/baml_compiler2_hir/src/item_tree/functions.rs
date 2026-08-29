use baml_base::Name;
use baml_compiler2_ast::ast;
use text_size::TextRange;

use crate::{
    ids::{FunctionMarker, LocalItemId},
    item_tree::GenericParam,
};

/// Full function data stored in the `ItemTree`.
/// Params and return type are stored for signature queries.
/// Body is stored for body queries (no CST re-parsing needed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Function {
    pub name: Name,
    /// Generic type parameters (e.g., `["T", "U"]`), each with its bounds.
    /// Empty for non-generic functions.
    pub generic_params: Vec<GenericParam>,
    /// Function parameters with optional type annotations and spans.
    pub params: Vec<FunctionParam>,
    /// Function parameter default expression arena.
    pub defaults: ast::FunctionDefaults,
    /// Return type with its source span.
    pub return_type: Option<ast::TypeExpr>,
    /// Throws contract type with its source span.
    pub throws: Option<ast::TypeExpr>,
    /// Function body — either an expression or a builtin.
    pub body: Option<ast::FunctionBodyDef>,
    /// Declarative metadata, if this function was declared with declarative syntax.
    pub declarative_meta: Option<ast::DeclarativeMeta>,
    pub metadata: ast::FunctionMetadata,
    /// Joined `///` doc-comment lines preceding this declaration.
    pub docstring: Option<String>,
    /// BEP-049 §10: set when the fn def had a `//baml:tagged_string` marker.
    /// Mirrors `ast::FunctionDef::is_tagged_template_tag` so TIR can validate
    /// tagged-template tags without re-reading the CST.
    pub is_tagged_template_tag: bool,
    /// Full source span of the function.
    pub span: TextRange,
}

/// The item a method belongs to.
///
/// Recorded by the `ItemTreeBuilder` at the same call that establishes
/// membership (`set_class_methods` / `alloc_interface` / `alloc_impl`), so it
/// cannot drift from the forward lists. Before this existed, ~24 sites across
/// TIR/MIR/emit/HIR answered "who owns this method?" by scanning
/// `classes.values().find(|c| c.methods.contains(&id))` and friends — O(items)
/// per lookup, and with the class/interface halves drifting between copies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MethodOwner {
    /// A class-LEVEL method only. A method declared inside an in-body
    /// `implements I { … }` block belongs to its impl block (`Impl`) — the
    /// in-class spelling is pure syntax.
    Class(LocalItemId<crate::ids::ClassMarker>),
    /// An interface default method.
    Interface(LocalItemId<crate::ids::InterfaceMarker>),
    /// A method of an `implements` block — in-class and out-of-body alike.
    Impl(LocalItemId<crate::ids::ImplMarker>),
}

/// A function parameter entry in the `ItemTree`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionParam {
    pub name: Name,
    pub type_expr: Option<ast::TypeExpr>,
    pub default: Option<DefaultExprRef>,
    pub span: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefaultExprRef {
    pub function: LocalItemId<FunctionMarker>,
    pub expr: ast::DefaultExprId,
}
