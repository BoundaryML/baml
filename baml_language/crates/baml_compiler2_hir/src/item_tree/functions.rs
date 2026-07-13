use baml_base::Name;
use baml_compiler2_ast::ast;
use text_size::TextRange;

use crate::ids::{FunctionMarker, LocalItemId};

/// Full function data stored in the `ItemTree`.
/// Params and return type are stored for signature queries.
/// Body is stored for body queries (no CST re-parsing needed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Function {
    pub name: Name,
    /// Generic type parameters (e.g., `["T", "U"]`).
    /// Empty for non-generic functions.
    pub generic_params: Vec<Name>,
    /// BEP-044 generic bounds parallel to `generic_params`. `Some(te)`
    /// means the parameter at the matching index was declared with
    /// `T extends <te>`; `None` means unbounded.
    pub generic_param_bounds: Vec<Option<ast::TypeExpr>>,
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
    pub origin: ast::FunctionOrigin,
    /// Joined `///` doc-comment lines preceding this declaration.
    pub docstring: Option<String>,
    /// BEP-049 §10: set when the fn def had a `//baml:tagged_string` marker.
    /// Mirrors `ast::FunctionDef::is_tagged_template_tag` so TIR can validate
    /// tagged-template tags without re-reading the CST.
    pub is_tagged_template_tag: bool,
    /// Full source span of the function.
    pub span: TextRange,
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
