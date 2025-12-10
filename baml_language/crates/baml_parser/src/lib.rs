//! Parsing BAML source into Rowan syntax trees.
//!
//! Implements incremental parsing with error recovery.

use baml_base::SourceFile;
use baml_diagnostics::compiler_error::parse_error::ParseError;
use baml_lexer::lex_file;
use baml_syntax::SyntaxNode;
use rowan::GreenNode;

mod parser;
pub use parser::{parse_file, parse_file_with_cache};

/// Tracked struct that holds both parse outputs together
#[salsa::tracked]
pub struct ParseResult<'db> {
    #[tracked]
    pub green: GreenNode,

    #[tracked]
    pub errors: Vec<ParseError>,
}

/// Tracked: parse file and return both green tree and errors.
///
/// Note: We can't make this take Vec<Token> directly because Salsa tracked
/// functions can only take Salsa-tracked types as input. So we take `SourceFile`,
/// call `lex_file` (tracked), then call `parse_file` (not tracked) with the tokens.
#[salsa::tracked]
pub fn parse_result(db: &dyn salsa::Database, file: SourceFile) -> ParseResult<'_> {
    let tokens = lex_file(db, file);
    let (green, errors) = parse_file(&tokens);
    ParseResult::new(db, green, errors)
}

/// Get the green tree from parsing a file
pub fn parse_green(db: &dyn salsa::Database, file: SourceFile) -> GreenNode {
    let result = parse_result(db, file);
    result.green(db)
}

/// Get parse errors from parsing a file
pub fn parse_errors(db: &dyn salsa::Database, file: SourceFile) -> Vec<ParseError> {
    let result = parse_result(db, file);
    result.errors(db)
}

/// Helper to build a red tree from the green tree.
pub fn syntax_tree(db: &dyn salsa::Database, file: SourceFile) -> SyntaxNode {
    let green = parse_green(db, file);
    SyntaxNode::new_root(green)
}
