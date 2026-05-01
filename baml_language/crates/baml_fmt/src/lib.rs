pub mod ast;
pub mod printer;
mod trivia_classifier;

use ast::FromCST as _;
use baml_db::{
    baml_compiler_diagnostics::ParseError,
    baml_compiler_lexer, baml_compiler_parser,
    baml_compiler_syntax::{SyntaxElement, SyntaxNode},
};
use baml_project::ProjectDatabase;
use printer::{Printer, Shape};
pub use trivia_classifier::{EmittableTrivia, TriviaInfo};

/// Runs the formatter on the given source code.
///
/// Also see [`format_salsa`] if you already have a [`salsa::Database`] with the source files in it.
///
/// # Errors
/// Errors can occur if the source code is invalid: the parser or AST errors will be returned.
pub fn format(source: &str, options: &FormatOptions) -> Result<String, FormatterError> {
    let mut db = ProjectDatabase::new();
    let source_file = db.add_file("file.baml", source);
    format_salsa(&db, source_file, options)
}

#[salsa::tracked]
pub fn format_salsa(
    db: &dyn salsa::Database,
    file: baml_db::SourceFile,
    options: &'_ FormatOptions,
) -> Result<String, FormatterError> {
    let tokens = baml_compiler_lexer::lex_file(db, file);
    let (parsed, errors) = baml_compiler_parser::parse_file(&tokens);
    if !errors.is_empty() {
        return Err(FormatterError::ParseErrors(errors));
    }

    let cst = SyntaxNode::new_root(parsed);
    let trivia = TriviaInfo::classify_trivia(&cst);
    let strong_ast = ast::SourceFile::from_cst(SyntaxElement::Node(cst))?;

    let mut printer = Printer::new_empty(file.text(db), options, &trivia);
    printer.print(
        &strong_ast,
        Shape {
            width: options.line_width,
            indent: 0,
            first_line_offset: 0,
        },
    );
    Ok(printer.output)
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

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FormatterError {
    #[error("{0:?}")]
    ParseErrors(Vec<ParseError>),
    #[error("{0}")]
    StrongAstError(#[from] ast::StrongAstError),
}

#[cfg(test)]
mod lambda_format_tests {
    use super::*;

    #[test]
    fn test_lambda_basic_formatting() {
        let source = "function test_annotated() -> int {\n    let double = (x: int) -> int { x * 2 }\n    double(21)\n}\n";
        let options = FormatOptions::default();
        let formatted =
            format(source, &options).expect("formatter should succeed on annotated lambda");
        // Verify idempotency
        let second = format(&formatted, &options).expect("formatter should be idempotent");
        assert_eq!(formatted, second, "formatter should be idempotent");
    }

    #[test]
    fn test_lambda_with_throws_formatting() {
        let source = "function test_throws() -> int {\n    let risky = (x: int) -> int throws string { x }\n    risky(42)\n}\n";
        let options = FormatOptions::default();
        let formatted =
            format(source, &options).expect("formatter should succeed on lambda with throws");
        let second = format(&formatted, &options).expect("formatter should be idempotent");
        assert_eq!(formatted, second, "formatter should be idempotent");
    }

    #[test]
    fn test_function_type_with_throws_formatting() {
        let source = "type Callback = (x: int) -> string throws never\n";
        let options = FormatOptions::default();
        let formatted =
            format(source, &options).expect("formatter should succeed on function-type throws");
        let second = format(&formatted, &options).expect("formatter should be idempotent");
        assert_eq!(formatted, second, "formatter should be idempotent");
    }

    #[test]
    fn test_top_level_function_with_throws_formatting() {
        let source = "function risky(x: int) -> int throws string {\n    throw \"boom\"\n}\n";
        let options = FormatOptions::default();
        let formatted =
            format(source, &options).expect("formatter should succeed on top-level throws");
        let second = format(&formatted, &options).expect("formatter should be idempotent");
        assert_eq!(formatted, second, "formatter should be idempotent");
    }

    #[test]
    fn test_generic_lambda_formatting() {
        let source = "function test_generic() -> int {\n    let identity = <T>(x: T) -> T { x }\n    identity(42)\n}\n";
        let options = FormatOptions::default();
        let formatted =
            format(source, &options).expect("formatter should succeed on generic lambda");
        let second = format(&formatted, &options).expect("formatter should be idempotent");
        assert_eq!(formatted, second, "formatter should be idempotent");
    }
}
