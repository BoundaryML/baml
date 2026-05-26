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
    format_salsa(&db, source_file, *options)
}

#[salsa::tracked]
#[allow(clippy::drop_non_drop)] // salsa macro expands to drop() of the args tuple
pub fn format_salsa(
    db: &dyn salsa::Database,
    file: baml_db::SourceFile,
    options: FormatOptions,
) -> Result<String, FormatterError> {
    let tokens = baml_compiler_lexer::lex_file(db, file);
    let (parsed, errors) = baml_compiler_parser::parse_file(&tokens);
    if !errors.is_empty() {
        return Err(FormatterError::ParseErrors(errors));
    }

    let cst = SyntaxNode::new_root(parsed);
    let trivia = TriviaInfo::classify_trivia(&cst);
    let strong_ast = ast::SourceFile::from_cst(SyntaxElement::Node(cst))?;

    let mut printer = Printer::new_empty(file.text(db), &options, &trivia);
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

#[cfg(test)]
mod pattern_format_tests {
    use super::*;

    /// Regression: an empty array pattern with a `: T` ascription used to
    /// drop the ascription because the empty-array fast path in
    /// `ArrayPattern::try_print_single_line` returned before the ascription
    /// branch ran. See PR #3509 review feedback.
    #[test]
    fn test_empty_array_pattern_with_ascription_preserves_ty() {
        let source = "function f(xs: int[]) -> int {\n    let []: int[] = xs;\n    0\n}\n";
        let options = FormatOptions::default();
        let formatted =
            format(source, &options).expect("formatter should succeed on empty array ascription");
        assert!(
            formatted.contains("[]: int[]"),
            "formatter dropped the `: int[]` ascription on an empty array pattern; got:\n{formatted}"
        );
        let second = format(&formatted, &options).expect("formatter should be idempotent");
        assert_eq!(formatted, second, "formatter should be idempotent");
    }

    /// Regression: trailing trivia after the statement-level `let` keyword
    /// (e.g. a comment between `let` and the pattern) used to be dropped
    /// because the printer normalized `let_keyword` to a literal space
    /// instead of emitting its trailing trivia. See PR #3509 review
    /// feedback.
    #[test]
    fn test_let_keyword_trailing_trivia_preserved() {
        let source = "function f(xs: int[]) -> int {\n    let /*keep*/ [x] = xs;\n    x\n}\n";
        let options = FormatOptions::default();
        let formatted = format(source, &options)
            .expect("formatter should succeed on let with trailing comment");
        assert!(
            formatted.contains("/*keep*/"),
            "formatter dropped the `/*keep*/` comment between `let` and the pattern; got:\n{formatted}"
        );
        let second = format(&formatted, &options).expect("formatter should be idempotent");
        assert_eq!(formatted, second, "formatter should be idempotent");
    }
}

#[cfg(test)]
mod is_format_tests {
    //! Formatter tests for `<expr> is <pattern>`. Each case rounds the
    //! source through `format` twice and asserts (a) the formatter
    //! succeeds, (b) the keyword + pattern stay on one line when the
    //! result fits, and (c) the formatter is idempotent.

    use super::*;

    fn assert_formats_to(source: &str, expected: &str) {
        let options = FormatOptions::default();
        let formatted = format(source, &options).expect("formatter should succeed on `is`");
        assert_eq!(
            formatted, expected,
            "formatter output didn't match expected\n--- got ---\n{formatted}\n--- want ---\n{expected}"
        );
        let second = format(&formatted, &options).expect("formatter should be idempotent");
        assert_eq!(formatted, second, "formatter should be idempotent");
    }

    #[test]
    fn test_is_basic_type_pattern_one_line() {
        let source = "function check(v: int | string) -> bool {\n    v is int\n}\n";
        assert_formats_to(source, source);
    }

    #[test]
    fn test_is_negation_stays_on_one_line() {
        // Regression: when `IS_EXPR` was treated as `Expression::Unknown`,
        // `!(v is string)` expanded across three lines. The dedicated
        // formatter path keeps it inline.
        let source = "function check(v: int | string) -> bool {\n    !(v is string)\n}\n";
        assert_formats_to(source, source);
    }

    #[test]
    fn test_is_inside_if_condition_stays_on_one_line() {
        // Same root cause as the negation case — the unknown-fallback used
        // to spread `if (v is int)` across multiple lines.
        let source = "function classify(v: int | string) -> string {\n    if (v is int) {\n        \"number\"\n    } else {\n        \"text\"\n    }\n}\n";
        assert_formats_to(source, source);
    }

    #[test]
    fn test_is_or_pattern_keeps_or_inline() {
        // The pattern side is rendered via `MatchPattern`'s printer, so
        // or-patterns stay on one line when they fit.
        let source = "function check(v: int | string | bool) -> bool {\n    v is int | bool\n}\n";
        assert_formats_to(source, source);
    }

    #[test]
    fn test_is_with_class_destructure_pattern() {
        // Use already-formatted class syntax (`: type,` form) since this
        // test is about the `is` expression, not class formatting.
        let source = "class User {\n    name: string,\n    age: int,\n}\n\nfunction is_user(u: User) -> bool {\n    u is User { name, age }\n}\n";
        assert_formats_to(source, source);
    }

    #[test]
    fn test_is_chained_with_and_stays_on_one_line() {
        let source = "function both(a: int | string, b: int | string) -> bool {\n    a is int && b is int\n}\n";
        assert_formats_to(source, source);
    }

    #[test]
    fn test_is_chained_on_itself_without_parens() {
        // Left-associative chain without parens — the formatter must not
        // insert any.
        let source = "function check(v: int | string) -> bool {\n    v is int is bool\n}\n";
        assert_formats_to(source, source);
    }

    #[test]
    fn test_is_literal_pattern() {
        let source = "function check(n: int) -> bool {\n    n is 0\n}\n";
        assert_formats_to(source, source);
    }

    // ── Trivia handling ─────────────────────────────────────────────────
    //
    // The single-line `is` printer mirrors `BinaryExpr`'s trivia rules:
    // block comments between the scrutinee / keyword / pattern are kept
    // verbatim, butted against neighboring tokens (no surrounding spaces),
    // and the explicit ` ` between scrutinee/keyword/pattern is only
    // emitted when there's no comment in that gap. The tests below pin
    // each case so a regression that drops the comment surfaces here.

    #[test]
    fn test_is_block_comment_between_scrutinee_and_keyword() {
        let source = "function check(v: int | string) -> bool {\n    v /* before */ is int\n}\n";
        let expected = "function check(v: int | string) -> bool {\n    v/* before */is int\n}\n";
        assert_formats_to(source, expected);
    }

    #[test]
    fn test_is_block_comment_between_keyword_and_pattern() {
        let source = "function check(v: int | string) -> bool {\n    v is /* after */ int\n}\n";
        let expected = "function check(v: int | string) -> bool {\n    v is/* after */int\n}\n";
        assert_formats_to(source, expected);
    }

    #[test]
    fn test_is_block_comments_on_both_sides_of_keyword() {
        // Both sides — confirms the keyword still appears between the two
        // comments instead of being swallowed or duplicated.
        let source = "function check(v: int | string) -> bool {\n    v /* a */ is /* b */ int\n}\n";
        let expected = "function check(v: int | string) -> bool {\n    v/* a */is/* b */int\n}\n";
        assert_formats_to(source, expected);
    }

    #[test]
    fn test_is_trailing_line_comment_after_pattern() {
        // A line comment after the pattern attaches as trailing trivia of
        // the enclosing statement/block, not of the `is` expression. The
        // expression itself still formats on one line; the comment is
        // preserved by the block-level printer.
        let source = "function check(v: int | string) -> bool {\n    v is int // a note\n}\n";
        assert_formats_to(source, source);
    }
}
