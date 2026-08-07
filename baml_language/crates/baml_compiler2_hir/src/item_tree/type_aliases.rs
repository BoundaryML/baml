use baml_base::Name;
use baml_compiler2_ast::ast;
use text_size::TextRange;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeAlias {
    pub name: Name,
    /// The type expression on the RHS of the alias, if present.
    pub type_expr: Option<ast::TypeExpr>,
    /// Full source span of the type alias declaration.
    pub span: TextRange,
    pub docstring: Option<String>,
}
