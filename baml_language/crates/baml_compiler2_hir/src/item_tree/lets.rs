use baml_base::Name;
use baml_compiler2_ast::ast;
use text_size::TextRange;

/// A top-level let binding stored in the `ItemTree`.
/// Carries the optional initializer `ExprBody` for body queries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Let {
    pub name: Name,
    pub initializer: Option<(ast::ExprBody, ast::AstSourceMap)>,
    pub origin: ast::LetOrigin,
    pub span: TextRange,
    pub name_span: TextRange,
}
