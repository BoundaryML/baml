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

    /// A `#!` shebang must survive formatting verbatim as the first line —
    /// otherwise `baml fmt` would silently break an executable `.baml`
    /// script. The shebang is treated as a leading line comment.
    #[test]
    fn test_shebang_preserved_as_first_line() {
        let source =
            "#!/usr/bin/env -S baml run --file\nfunction main() -> string {\n    \"hi\"\n}\n";
        let options = FormatOptions::default();
        let formatted = format(source, &options).expect("formatter should accept a shebang");
        assert!(
            formatted.starts_with("#!/usr/bin/env -S baml run --file\n"),
            "shebang must remain the literal first line, got:\n{formatted}"
        );
        let second = format(&formatted, &options).expect("formatter should be idempotent");
        assert_eq!(formatted, second, "formatter should be idempotent");
    }

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
    fn test_top_level_interface_spacing_is_idempotent() {
        let source = r#"interface Named {
  type Key = string
  function label(self) -> string
}

interface Printable {
  function display(self) -> string
}

class Ticket {
  id: string
  function label(self) -> string { return self.id }
  implements Named {}
}

class Box<T> {
  value: T
}

implements<T extends Named> Printable for Box<T> {
  function display(self) -> string {
    return self.value.label()
  }
}
"#;
        let options = FormatOptions::default();
        let formatted =
            format(source, &options).expect("formatter should succeed on interface syntax");
        let second = format(&formatted, &options).expect("formatter should be idempotent");
        assert_eq!(formatted, second, "formatter should be idempotent");
        assert!(
            !formatted.contains("\n\n\n"),
            "top-level formatting should not grow multiple blank lines:\n{formatted}"
        );
    }

    /// Generic instantiation as a value (`foo<int>`) formats without error and is
    /// idempotent. Only a path base is a valid instantiation target; a generic
    /// *lambda* and a parenthesized base (`(foo)<int>`) are rejected by the
    /// compiler, so they are not exercised here.
    #[test]
    fn test_generic_instantiation_formatting() {
        let options = FormatOptions::default();
        let cases = [
            "function f() -> int {\n    let g = foo<int>\n    g(5)\n}\n",
            "function f() -> int {\n    foo<int>(5)\n}\n",
            "function f() -> int {\n    let g = a.b.foo<int, string>\n    5\n}\n",
        ];
        for source in cases {
            let formatted = format(source, &options).unwrap_or_else(|e| {
                panic!("formatter must not error on valid syntax: {e:?}\nsource:\n{source}")
            });
            let second = format(&formatted, &options).expect("formatter should be idempotent");
            assert_eq!(
                formatted, second,
                "formatter should be idempotent for:\n{source}"
            );
        }
    }
}

#[cfg(test)]
mod backtick_format_tests {
    use super::*;

    /// BEP-049: backtick string literals round-trip through the formatter. A
    /// multi-line interior is re-indented to sit one level past the surrounding
    /// block, but ONLY when that is provably value-preserving: a backtick string
    /// is auto-dedented at lower time (BEP-049 §12), so the formatter strips the
    /// same common prefix, re-emits at the block indent, and verifies the runtime
    /// value is unchanged before applying. Single-line literals, tick ladders,
    /// `${for}`/`${if}` block tags, and multi-line interpolations stay verbatim.
    #[test]
    fn backtick_one_liner_round_trips() {
        let source = "function Demo() -> string {\n    `hello ${name} world`\n}\n";
        let options = FormatOptions::default();
        let formatted = format(source, &options).expect("formatter should succeed");
        assert!(
            formatted.contains("`hello ${name} world`"),
            "got: {formatted}"
        );
        let second = format(&formatted, &options).expect("formatter should be idempotent");
        assert_eq!(formatted, second);
    }

    #[test]
    fn backtick_multi_tick_ladder_round_trips() {
        let source = "function Demo() -> string {\n    ``inline `code` here``\n}\n";
        let options = FormatOptions::default();
        let formatted = format(source, &options).expect("formatter should succeed");
        assert!(
            formatted.contains("``inline `code` here``"),
            "got: {formatted}"
        );
        let second = format(&formatted, &options).expect("formatter should be idempotent");
        assert_eq!(formatted, second);
    }

    #[test]
    fn backtick_multiline_round_trips() {
        let source =
            "function Demo() -> string {\n    `\n        line one\n        line two\n    `\n}\n";
        let options = FormatOptions::default();
        let formatted = format(source, &options).expect("formatter should succeed");
        assert!(formatted.contains("line one"));
        assert!(formatted.contains("line two"));
        let second = format(&formatted, &options).expect("formatter should be idempotent");
        assert_eq!(formatted, second);
    }

    /// The reported bug: a backtick string as a `prompt:` value used to make
    /// `baml fmt` bail on the whole file. It is now accepted, and its
    /// over-indented interior is re-indented to one level past the field (8
    /// spaces), dedented by the common leading indent.
    #[test]
    fn backtick_prompt_multiline_dedents() {
        let source = "function Demo(name: string) -> string {\n    client \"openai/gpt-4o\"\n    prompt `\n            Hello ${name}\n            Goodbye\n    `\n}\n";
        let expected = "function Demo(name: string) -> string {\n    client: \"openai/gpt-4o\"\n    prompt: `\n        Hello ${name}\n        Goodbye\n    `\n}\n";
        let options = FormatOptions::default();
        let formatted = format(source, &options).expect("formatter should succeed");
        assert_eq!(formatted, expected, "got:\n{formatted}");
        let second = format(&formatted, &options).expect("formatter should be idempotent");
        assert_eq!(formatted, second);
    }

    /// A single-line backtick prompt is accepted and printed verbatim.
    #[test]
    fn backtick_prompt_one_liner_accepted() {
        let source = "function Demo() -> string {\n    client \"openai/gpt-4o\"\n    prompt `Just one line`\n}\n";
        let expected = "function Demo() -> string {\n    client: \"openai/gpt-4o\"\n    prompt: `Just one line`\n}\n";
        let options = FormatOptions::default();
        let formatted = format(source, &options).expect("formatter should succeed");
        assert_eq!(formatted, expected, "got:\n{formatted}");
        let second = format(&formatted, &options).expect("formatter should be idempotent");
        assert_eq!(formatted, second);
    }

    /// A backtick string is accepted as an attribute argument and re-indented
    /// relative to the wrapped attribute layout.
    #[test]
    fn backtick_attribute_arg_dedents() {
        let source = "class Foo {\n    bar string @description(`\n        some desc\n        more\n    `)\n}\n";
        let expected = "class Foo {\n    bar: string @description(\n        `\n            some desc\n            more\n        `,\n    ),\n}\n";
        let options = FormatOptions::default();
        let formatted = format(source, &options).expect("formatter should succeed");
        assert_eq!(formatted, expected, "got:\n{formatted}");
        let second = format(&formatted, &options).expect("formatter should be idempotent");
        assert_eq!(formatted, second);
    }

    /// A backtick `template_string` body is accepted and its interior re-indented
    /// (closing backtick at column 0, like a raw-string body).
    #[test]
    fn backtick_template_string_dedents() {
        let source = "template_string Foo(name: string) `\n        Hello ${name}\n        Bye\n`\n";
        let expected = "template_string Foo(name: string) `\n    Hello ${name}\n    Bye\n`\n";
        let options = FormatOptions::default();
        let formatted = format(source, &options).expect("formatter should succeed");
        assert_eq!(formatted, expected, "got:\n{formatted}");
        let second = format(&formatted, &options).expect("formatter should be idempotent");
        assert_eq!(formatted, second);
    }

    /// Backtick string in expression position re-indents its over-indented
    /// interior to the surrounding block.
    #[test]
    fn backtick_expr_multiline_dedents() {
        let source = "function Demo() -> string {\n    let x = `\n            line one\n            line two\n    `;\n    x\n}\n";
        let expected = "function Demo() -> string {\n    let x = `\n        line one\n        line two\n    `;\n    x\n}\n";
        let options = FormatOptions::default();
        let formatted = format(source, &options).expect("formatter should succeed");
        assert_eq!(formatted, expected, "got:\n{formatted}");
        let second = format(&formatted, &options).expect("formatter should be idempotent");
        assert_eq!(formatted, second);
    }

    /// Value preservation: the first content line is less-indented than the
    /// second, so under §12 dedent the second line's extra indent is part of the
    /// string's value. The re-indent moves the block to the canonical position
    /// but preserves that relative indent (second line stays 6 spaces deeper).
    #[test]
    fn backtick_relative_indent_preserved() {
        let source =
            "function D() -> string {\n    let x = `first line\n      second line`;\n    x\n}\n";
        let expected = "function D() -> string {\n    let x = `\n        first line\n              second line\n    `;\n    x\n}\n";
        let options = FormatOptions::default();
        let formatted = format(source, &options).expect("formatter should succeed");
        assert_eq!(formatted, expected, "got:\n{formatted}");
        let second = format(&formatted, &options).expect("formatter should be idempotent");
        assert_eq!(formatted, second);
    }

    /// A `${for}`/`${if}` block tag triggers §13 whitespace control, so the
    /// interior is left verbatim (re-indenting could change the value).
    #[test]
    fn backtick_block_tag_stays_verbatim() {
        let source = "function F(xs: string[]) -> string {\n    `${for (let x in xs)}- ${x}\n${endfor}`\n}\n";
        let options = FormatOptions::default();
        let formatted = format(source, &options).expect("formatter should succeed");
        assert!(
            formatted.contains("`${for (let x in xs)}- ${x}\n${endfor}`"),
            "block-tag template must stay verbatim, got:\n{formatted}"
        );
        let second = format(&formatted, &options).expect("formatter should be idempotent");
        assert_eq!(formatted, second);
    }

    /// A multi-line `${...}` interpolation is placeholdered before the runtime
    /// §12 min-indent (its inner lines are not re-indented), so the formatter
    /// leaves the whole literal verbatim rather than risk changing the value.
    #[test]
    fn backtick_multiline_interp_stays_verbatim() {
        let source = "function D() -> string {\n    let x = `\n        a ${\n            foo()\n        } b\n    `;\n    x\n}\n";
        let options = FormatOptions::default();
        let formatted = format(source, &options).expect("formatter should succeed");
        assert!(
            formatted.contains("`\n        a ${\n            foo()\n        } b\n    `"),
            "multi-line interp must stay verbatim, got:\n{formatted}"
        );
        let second = format(&formatted, &options).expect("formatter should be idempotent");
        assert_eq!(formatted, second);
    }

    /// A single-line backtick is never split, even when it is longer than the
    /// line width: wrapping it would insert a newline and change its value.
    #[test]
    fn backtick_single_line_over_width_not_wrapped() {
        let long = "x".repeat(160);
        let source = format!("function D(name: string) -> string {{\n    `{long} ${{name}}`\n}}\n");
        let options = FormatOptions::default();
        let formatted = format(&source, &options).expect("formatter should succeed");
        assert!(
            formatted.contains(&format!("    `{long} ${{name}}`\n")),
            "single-line backtick must stay on one line, got:\n{formatted}"
        );
        let second = format(&formatted, &options).expect("formatter should be idempotent");
        assert_eq!(formatted, second);
    }
}

#[cfg(test)]
mod linear_formatter_regression_tests {
    use super::*;

    #[test]
    fn keyword_path_segments_format() {
        let source = "function repro() -> int {\n    let g = baml.spawn.TaskGroup.new(2, name = \"fmt-repro\");\n    let f = spawn with baml.spawn.options(group = g) {\n        42\n    };\n    await f\n}\n";
        let options = FormatOptions::default();
        let formatted = format(source, &options)
            .expect("formatter should accept keyword path segments after `.`");

        assert!(
            formatted.contains("baml.spawn.TaskGroup.new")
                && formatted.contains("baml.spawn.options"),
            "keyword path segments should round-trip, got:\n{formatted}"
        );
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
mod catch_format_tests {
    use super::*;

    #[test]
    fn test_catch_arm_bodies_indent_inside_enclosing_block() {
        let source = r#"function demo(s: string) -> int {
    baml.json.from_string<int>(s) catch (e) {
    baml.json.JsonParseError => 0,
    baml.json.JsonDecodeError => 0,
  };
    42
}
"#;
        let expected = r#"function demo(s: string) -> int {
    baml.json.from_string<int>(s) catch (e) {
        baml.json.JsonParseError => 0,
        baml.json.JsonDecodeError => 0,
    };
    42
}
"#;
        let options = FormatOptions::default();
        let formatted = format(source, &options).expect("formatter should succeed on catch arms");

        assert_eq!(formatted, expected);
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

#[cfg(test)]
mod if_let_format_tests {
    //! Formatter tests for `if let PATTERN = SCRUTINEE { ... } else { ... }`.

    use super::*;

    fn assert_formats_to(source: &str, expected: &str) {
        let options = FormatOptions::default();
        let formatted = format(source, &options).expect("formatter should succeed on `if let`");
        assert_eq!(
            formatted, expected,
            "formatter output didn't match expected\n--- got ---\n{formatted}\n--- want ---\n{expected}"
        );
        let second = format(&formatted, &options).expect("formatter should be idempotent");
        assert_eq!(formatted, second, "formatter should be idempotent");
    }

    #[test]
    fn test_if_let_basic_binding() {
        let source = "function f(r: int | string) -> string {\n    if let v: int = r {\n        \"int\"\n    } else {\n        \"other\"\n    }\n}\n";
        assert_formats_to(source, source);
    }

    #[test]
    fn test_if_let_no_else() {
        // Statement form — no else branch, expression evaluates to void.
        let source = "function f(r: int | string) -> void {\n    if let v: int = r {\n        let _ = v;\n    }\n}\n";
        assert_formats_to(source, source);
    }

    #[test]
    fn test_if_let_else_if_let_chain() {
        // `else if let` should preserve the chain shape on one statement.
        let source = "function f(r: int | string | bool) -> string {\n    if let v: int = r {\n        \"int\"\n    } else if let v: string = r {\n        v\n    } else {\n        \"bool\"\n    }\n}\n";
        assert_formats_to(source, source);
    }

    #[test]
    fn test_if_let_else_if_plain() {
        // Mixing `if let` with a plain `else if` (using `is`) must format
        // cleanly. Tests the IF_EXPR-after-IF_LET_EXPR else branch.
        // Plain `if` conditions are canonicalised with parens by the
        // formatter, hence the `if (r is string)` in the expected output.
        let source = "function f(r: int | string) -> string {\n    if let v: int = r {\n        \"int\"\n    } else if r is string {\n        \"str\"\n    } else {\n        \"other\"\n    }\n}\n";
        let expected = "function f(r: int | string) -> string {\n    if let v: int = r {\n        \"int\"\n    } else if (r is string) {\n        \"str\"\n    } else {\n        \"other\"\n    }\n}\n";
        assert_formats_to(source, expected);
    }

    #[test]
    fn test_if_let_destructure_pattern() {
        // Class destructure pattern inside if-let.
        let source = "class User {\n    name: string,\n    age: int,\n}\n\nfunction f(u: User) -> string {\n    if let User { name, age } = u {\n        name\n    } else {\n        \"none\"\n    }\n}\n";
        assert_formats_to(source, source);
    }

    #[test]
    fn test_if_let_or_pattern() {
        let source = "class Ok {\n    value: string,\n}\n\nclass Warn {\n    value: string,\n}\n\nfunction f(r: Ok | Warn) -> string {\n    if let s: Ok | Warn = r {\n        s.value\n    } else {\n        \"none\"\n    }\n}\n";
        assert_formats_to(source, source);
    }
}

#[cfg(test)]
mod let_else_format_tests {
    //! Formatter tests for `let PATTERN = SCRUTINEE else { ... };`.

    use super::*;

    fn assert_formats_to(source: &str, expected: &str) {
        let options = FormatOptions::default();
        let formatted = format(source, &options).expect("formatter should succeed on `let … else`");
        assert_eq!(
            formatted, expected,
            "formatter output didn't match expected\n--- got ---\n{formatted}\n--- want ---\n{expected}"
        );
        let second = format(&formatted, &options).expect("formatter should be idempotent");
        assert_eq!(formatted, second, "formatter should be idempotent");
    }

    #[test]
    fn test_let_else_basic() {
        // The else block is a BlockExpr — formatter canonicalises it to
        // multi-line layout, matching the rest of the codebase's block
        // style.
        let source = "function f(r: int | string) -> int {\n    let v: int = r else {\n        return 0;\n    };\n    v\n}\n";
        assert_formats_to(source, source);
    }

    #[test]
    fn test_let_else_destructure() {
        let source = "class User {\n    name: string,\n    age: int,\n}\n\nclass Admin {\n    handle: string,\n}\n\nfunction f(u: User | Admin) -> string {\n    let User { name, age } = u else {\n        return \"admin\";\n    };\n    name\n}\n";
        assert_formats_to(source, source);
    }

    #[test]
    fn test_let_else_or_pattern() {
        let source = "class Ok {\n    value: string,\n}\n\nclass Warn {\n    value: string,\n}\n\nclass Err {\n    message: string,\n}\n\nfunction f(r: Ok | Warn | Err) -> string {\n    let s: Ok | Warn = r else {\n        return \"err\";\n    };\n    s.value\n}\n";
        assert_formats_to(source, source);
    }

    #[test]
    fn test_let_else_throw() {
        // `throw` in the else branch is a valid diverging form. Empty
        // class body renders multi-line by the same block-expander rule.
        let source = "class NoMatch {\n}\n\nfunction f(r: int | string) -> int throws NoMatch {\n    let n: int = r else {\n        throw NoMatch {};\n    };\n    n\n}\n";
        assert_formats_to(source, source);
    }

    #[test]
    fn test_let_else_preserves_trivia_around_else_keyword() {
        // Trivia between the initializer and `else` (and between `else`
        // and the block) must round-trip — pre-fix the formatter emitted
        // hardcoded spaces and dropped any adjacent comments. The
        // formatter squishes whitespace around the comments but keeps
        // the comments themselves; the output is idempotent.
        let source = "function f(r: int | string) -> int {\n    let v: int = r /* a */ else /* b */ {\n        return 0;\n    };\n    v\n}\n";
        let expected = "function f(r: int | string) -> int {\n    let v: int = r/* a */ else /* b */{\n        return 0;\n    };\n    v\n}\n";
        assert_formats_to(source, expected);
    }

    #[test]
    fn test_plain_let_unchanged() {
        // Regression: plain `let x = …;` without an else clause must still
        // format cleanly without picking up a stray `else { … }` tail.
        let source = "function f() -> int {\n    let x: int = 1;\n    x\n}\n";
        assert_formats_to(source, source);
    }
}

#[cfg(test)]
mod while_let_format_tests {
    //! Formatter tests for `while let PATTERN = SCRUTINEE { ... }`.

    use super::*;

    fn assert_formats_to(source: &str, expected: &str) {
        let options = FormatOptions::default();
        let formatted = format(source, &options).expect("formatter should succeed on `while let`");
        assert_eq!(
            formatted, expected,
            "formatter output didn't match expected\n--- got ---\n{formatted}\n--- want ---\n{expected}"
        );
        let second = format(&formatted, &options).expect("formatter should be idempotent");
        assert_eq!(formatted, second, "formatter should be idempotent");
    }

    #[test]
    fn test_while_let_basic() {
        // No parens around the scrutinee (mirrors `if let`, unlike plain
        // `while` which canonicalises parens); the body canonicalises to a
        // multi-line block, and there is no trailing semicolon.
        let source = "function f(r: int | null) -> int {\n    while let v: int = r {\n        break;\n    }\n    0\n}\n";
        assert_formats_to(source, source);
    }

    #[test]
    fn test_while_let_destructure() {
        let source = "class Item {\n    value: int,\n}\n\nfunction f(it: Item | null) -> int {\n    while let Item { value } = it {\n        break;\n    }\n    0\n}\n";
        assert_formats_to(source, source);
    }

    #[test]
    fn test_while_let_array_pattern() {
        // Array-pattern head: the parser keeps `let` as a statement-level token
        // (outside the PATTERN node), so the formatter must round-trip it via
        // the optional `let_keyword` field. Before the fix, `from_cst` errored
        // here because it expected a PATTERN right after `while`.
        let options = FormatOptions::default();
        let source = "function f(xs: int[]) -> int {\n    while let [a, b] = xs {\n        break;\n    }\n    0\n}\n";
        let formatted = format(source, &options)
            .expect("formatter should succeed on a while-let array-pattern head");
        assert!(
            formatted.contains("while let [") && formatted.contains("= xs"),
            "while-let array-pattern head should round-trip with its `let`; got:\n{formatted}"
        );
        let second = format(&formatted, &options).expect("formatter should be idempotent");
        assert_eq!(formatted, second, "formatter should be idempotent");
    }

    #[test]
    fn test_plain_while_unchanged() {
        // Regression: plain `while (cond) { … }` keeps its parens and must
        // not pick up while-let formatting.
        let source =
            "function f(b: bool) -> int {\n    while (b) {\n        break;\n    }\n    0\n}\n";
        assert_formats_to(source, source);
    }
}

#[cfg(test)]
mod const_format_tests {
    use super::*;

    fn assert_formats_to(source: &str, expected: &str) {
        let options = FormatOptions::default();
        let formatted = format(source, &options).expect("formatter should succeed on `const`");
        assert_eq!(
            formatted, expected,
            "formatter output didn't match expected\n--- got ---\n{formatted}\n--- want ---\n{expected}"
        );
        let second = format(&formatted, &options).expect("formatter should be idempotent");
        assert_eq!(formatted, second, "formatter should be idempotent");
    }

    #[test]
    fn test_const_basic_binding() {
        let source = "function f() -> int {\n    const x = 1;\n    x\n}\n";
        assert_formats_to(source, source);
    }

    #[test]
    fn test_const_typed_binding() {
        let source = "function f() -> int {\n    const x: int = 1;\n    x\n}\n";
        assert_formats_to(source, source);
    }

    #[test]
    fn test_const_array_pattern() {
        let source = "function f(xs: int[]) -> int {\n    const [x] = xs;\n    x\n}\n";
        assert_formats_to(source, source);
    }

    #[test]
    fn test_if_const_binding() {
        let source = "function f(value: int | string) -> int {\n    if const x: int = value {\n        x\n    } else {\n        0\n    }\n}\n";
        assert_formats_to(source, source);
    }

    #[test]
    fn test_while_const_binding() {
        let source = "function f(value: int | string) -> int {\n    while const x: int = value {\n        break;\n    }\n    0\n}\n";
        assert_formats_to(source, source);
    }

    #[test]
    fn test_for_const_c_style() {
        let source = "function f() -> int {\n    const sum = 0;\n    for (const i = 0; i < 3; i += 1) {\n        sum += i;\n    }\n    sum\n}\n";
        assert_formats_to(source, source);
    }

    #[test]
    fn test_for_const_iterator() {
        let source = "function f(items: int[]) -> int {\n    const sum = 0;\n    for (const item in items) {\n        sum += item;\n    }\n    sum\n}\n";
        assert_formats_to(source, source);
    }

    #[test]
    fn test_const_binding_keyword_trailing_trivia_preserved() {
        let source = "function f() -> int {\n    const /*keep*/ x = 1;\n    x\n}\n";
        assert_formats_to(source, source);
    }

    #[test]
    fn test_const_wildcard_keyword_trailing_trivia_preserved() {
        let source = "function f() -> int {\n    const /*keep*/ _ = 1;\n    0\n}\n";
        assert_formats_to(source, source);
    }

    #[test]
    fn test_const_destructure_keyword_trailing_trivia_preserved() {
        let source = "class Foo {\n    a: int,\n}\n\nfunction f(foo: Foo) -> int {\n    const /*keep*/ Foo { a } = foo;\n    a\n}\n";
        assert_formats_to(source, source);
    }

    #[test]
    fn test_while_const_array_keyword_trailing_trivia_preserved() {
        let source = "function f(xs: int[]) -> int {\n    while const /*keep*/ [x] = xs {\n        break;\n    }\n    0\n}\n";
        assert_formats_to(source, source);
    }
}

#[cfg(test)]
mod map_literal_format_tests {
    //! Formatter tests for map literals, focused on the empty-map case.
    //! A non-empty map renders with interior padding (`{ "k": 1 }`), but an
    //! *empty* map must collapse to `{}` with no padding — the printer used to
    //! emit `{  }` because it added the leading/trailing space unconditionally.

    use super::*;

    fn assert_formats_to(source: &str, expected: &str) {
        let options = FormatOptions::default();
        let formatted = format(source, &options).expect("formatter should succeed on map literal");
        assert_eq!(
            formatted, expected,
            "formatter output didn't match expected\n--- got ---\n{formatted}\n--- want ---\n{expected}"
        );
        let second = format(&formatted, &options).expect("formatter should be idempotent");
        assert_eq!(formatted, second, "formatter should be idempotent");
    }

    #[test]
    fn test_empty_map_has_no_interior_padding() {
        // Regression for B-234: an empty map literal must format as `{}`, not `{  }`.
        let source = "function f() -> int {\n    {};\n    0\n}\n";
        assert_formats_to(source, source);
    }

    #[test]
    fn test_non_empty_map_keeps_interior_padding() {
        // Guard the established behavior: non-empty maps keep `{ ... }` padding.
        let source = "function f() -> int {\n    { \"a\": 1 };\n    0\n}\n";
        assert_formats_to(source, source);
    }

    #[test]
    fn test_empty_map_with_block_comment_keeps_padding() {
        // An interior comment is real content, so the padding stays.
        let source = "function f() -> int {\n    { /* keep */ };\n    0\n}\n";
        assert_formats_to(source, source);
    }
}

#[cfg(test)]
mod over_split_regression_tests {
    //! Regression tests for B-231: `baml fmt` used to force-wrap expressions
    //! that fit the line-width budget whenever they contained a node the
    //! strong AST doesn't model (`await`, `spawn`, the `.as<T>` projection,
    //! `throw`, bigint literals, …). Those nodes are held as
    //! [`crate::ast::Expression::Unknown`] and printed verbatim, but they used
    //! to report `single_line_width = None` and `multi_lined = true`
    //! unconditionally — which poisoned every *enclosing* expression's
    //! single-line attempt, exploding concise one-liners into deeply-indented
    //! blocks. The fix makes `Unknown` report its shape honestly from the raw
    //! source text, so a single-line unknown node stays inline like any other
    //! expression that fits.

    use super::*;

    fn assert_formats_to(source: &str, expected: &str) {
        let options = FormatOptions::default();
        let formatted = format(source, &options).expect("formatter should succeed");
        assert_eq!(
            formatted, expected,
            "formatter output didn't match expected\n--- got ---\n{formatted}\n--- want ---\n{expected}"
        );
        let second = format(&formatted, &options).expect("formatter should be idempotent");
        assert_eq!(formatted, second, "formatter should be idempotent");
    }

    #[test]
    fn test_await_in_paren_and_binary_stays_inline() {
        // The headline case from the ticket: `"got " + (await f)` was blown up
        // into a 5-line block with `await f` alone inside triple-indented parens.
        let source = "function main() -> string {\n    let f = spawn { compute() };\n    \"got \" + (await f)\n}\n\nfunction compute() -> string {\n    \"x\"\n}\n";
        assert_formats_to(source, source);
    }

    #[test]
    fn test_await_in_binary_no_string_stays_inline() {
        let source = "function main() -> int {\n    let f = spawn { 1 };\n    1 + (await f)\n}\n";
        assert_formats_to(source, source);
    }

    #[test]
    fn test_as_projection_field_access_stays_inline() {
        // `self.as<Dog>.name` (a `.as<T>` projection followed by a field access)
        // used to split across two lines because the projection is an unmodeled
        // node sitting at the head of the access chain.
        let source = "class Animal {\n    function name(self) -> string throws never {\n        self.as<Dog>.name\n    }\n}\n";
        assert_formats_to(source, source);
    }

    #[test]
    fn test_nested_call_chain_with_bigint_stays_inline() {
        // A bigint literal (`100n`) is also an unmodeled node. Nested inside a
        // call chain it used to force the whole chain to fan out across ~9 lines.
        let source = "function f() -> int throws never {\n    baml.sys.sleep(baml.time.Duration.from_milliseconds(100n))\n}\n";
        assert_formats_to(source, source);
    }

    #[test]
    fn test_braceless_throw_arm_stays_braceless() {
        // A braceless `=> throw …,` catch arm that fits the budget must not be
        // wrapped into a `=> { throw … }` block. `throw` is an unmodeled node, so
        // it used to report itself as multi-line and force the block wrap.
        let source = "function f(s: string) -> int throws Boom {\n    baml.json.from_string<int>(s) catch (e) {\n        baml.json.JsonParseError => throw Boom {},\n        baml.json.JsonDecodeError => 0,\n    }\n}\n\nclass Boom {\n}\n";
        assert_formats_to(source, source);
    }
}
