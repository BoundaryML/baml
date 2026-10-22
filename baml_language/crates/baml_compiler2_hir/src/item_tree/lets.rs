use baml_base::Name;
use baml_compiler2_ast::ast;
use text_size::TextRange;

/// A top-level let binding stored in the `ItemTree`.
/// Carries the optional initializer `ExprBody` for body queries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Let {
    pub name: Name,
    /// The written `: T`, if any. See [`ast::LetDef::type_annotation`].
    pub type_annotation: Option<ast::TypeExpr>,
    pub initializer: Option<(ast::ExprBody, ast::AstSourceMap)>,
    pub origin: ast::LetOrigin,
    pub docstring: Option<String>,
    pub span: TextRange,
    pub name_span: TextRange,
}
