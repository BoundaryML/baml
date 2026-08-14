use baml_base::Name;
use baml_compiler2_ast::ast;
use text_size::TextRange;

use crate::{
    ids::{FunctionMarker, LocalItemId},
    item_tree::{Attribute, GenericParam, ImplementsBlock},
};

/// A class field stored in the `ItemTree`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassField {
    pub name: Name,
    /// Always present — see [`ast::FieldDef::type_expr`]. A field written without a
    /// type recovers as `TypeExprKind::Error`, not as an absent type.
    pub type_expr: ast::TypeExpr,
    pub attributes: Vec<Attribute>,
    /// Joined `///` doc-comment lines preceding this declaration.
    pub docstring: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Class {
    pub name: Name,
    /// Generic type parameters (e.g., `["T"]` for `Array<T>`), each with its
    /// bounds. Empty for non-generic classes.
    pub generic_params: Vec<GenericParam>,
    /// Fields of the class, in declaration order.
    pub fields: Vec<ClassField>,
    /// Methods defined inside this class, referencing their `Function` entries
    /// in the same `ItemTree`. Includes both class-level methods and methods
    /// declared inside `implements I { ... }` blocks (BEP-044) — flattened so
    /// downstream code (e.g. signature queries, method dispatch) can iterate
    /// uniformly.
    pub methods: Vec<LocalItemId<FunctionMarker>>,
    /// `implements I { ... }` blocks, in declaration order. Each block keeps
    /// the raw target `TypeExpr` so generic parameters like `Container<int>`
    /// survive name resolution, plus field redeclarations from the block.
    pub implements: Vec<ImplementsBlock>,
    /// Block-level attributes (@@description, @@alias, etc.).
    pub attributes: Vec<Attribute>,
    /// Joined `///` doc-comment lines preceding this declaration.
    pub docstring: Option<String>,
    /// Full source span of the class declaration.
    pub span: TextRange,
}
