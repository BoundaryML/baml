pub mod ast;
mod trivia_classifier;

use baml_compiler_diagnostics::ParseError;
use baml_compiler_syntax::SyntaxNode;
use baml_project::ProjectDatabase;

use rowan::ast::AstNode;
pub use trivia_classifier::{EmittableTrivia, classify_trivia};

pub fn format(source: &str, options: &FormatOptions) -> Result<String, Vec<ParseError>> {
    let mut db = ProjectDatabase::new();
    let source_file = db.add_file("file.baml", source);
    format_salsa(&db, source_file, options.clone())
}

#[salsa::tracked]
pub fn format_salsa(
    db: &dyn salsa::Database,
    file: baml_base::SourceFile,
    options: FormatOptions,
) -> Result<String, Vec<ParseError>> {
    let tokens = baml_compiler_lexer::lex_file(db, file);
    let (parsed, errors) = baml_compiler_parser::parse_file(&tokens);
    if !errors.is_empty() {
        return Err(errors);
    }

    let ast = SyntaxNode::new_root(parsed);
    let file_root: baml_compiler_syntax::SourceFile = AstNode::cast(ast).unwrap();
    let trivia = classify_trivia(&file_root);
    todo!()
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FormatOptions {
    /// Maximum line width before wrapping kicks in. Default: `100`
    pub line_width: usize,
    /// Indent width. Default: `4`
    pub indent_width: usize,
}
impl Default for FormatOptions {
    fn default() -> Self {
        Self {
            line_width: 100,
            indent_width: 4,
        }
    }
}
