pub mod ast;
pub mod printer;
mod trivia_classifier;

use ast::FromCST as _;
use baml_db::{
    ProjectDatabase, SourceRootSpec,
    baml_compiler_diagnostics::ParseError,
    baml_compiler_lexer, baml_compiler_parser,
    baml_compiler_syntax::{SyntaxElement, SyntaxNode},
};
use printer::{Printer, Shape};
pub use trivia_classifier::{EmittableTrivia, TriviaInfo};

#[cfg(test)]
mod formatter_scenario_tests;

/// Runs the formatter on the given source code.
///
/// Also see [`format_salsa`] if you already have a [`salsa::Database`] with the source files in it.
///
/// # Errors
/// Errors can occur if the source code is invalid: the parser or AST errors will be returned.
pub fn format(source: &str, options: &FormatOptions) -> Result<String, FormatterError> {
    let (db, source_file) = single_file_db("file.baml", source);
    format_salsa(&db, source_file, *options)
}

/// A throwaway database holding exactly one workspace file.
///
/// Formatting is purely syntactic (lexer + parser over one file), so the
/// database carries no stdlib — only the workspace root the file must belong
/// to. The root's virtual path never touches the filesystem.
pub(crate) fn single_file_db(name: &str, source: &str) -> (ProjectDatabase, baml_db::SourceFile) {
    let mut db = ProjectDatabase::new();
    let root = db
        .add_source_root(SourceRootSpec {
            path: std::path::PathBuf::from("<fmt>"),
            package: baml_db::Name::new(baml_type::RESERVED_USER_PACKAGE),
            kind: baml_db::SourceRootKind::Workspace,
        })
        .unwrap_or_else(|e| unreachable!("fresh database accepts one workspace root: {e}"));
    let file =
        db.add_or_update_file_in(root, &std::path::PathBuf::from("<fmt>").join(name), source);
    (db, file)
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
    let cst = SyntaxNode::new_root(parsed);

    // Honor the documented escape hatch at the shared library boundary so
    // CLI and LSP behavior cannot diverge. Inspect parser-classified comment
    // tokens rather than the raw source: directive-like text inside a string
    // must not disable formatting. This check intentionally precedes the parse
    // error gate so the directive can protect an incomplete or intentionally
    // non-canonical file.
    if has_ignore_directive(&cst) {
        return Ok(file.text(db).clone());
    }

    if !errors.is_empty() {
        return Err(FormatterError::ParseErrors(errors));
    }

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

fn has_ignore_directive(cst: &SyntaxNode) -> bool {
    cst.descendants_with_tokens().any(|element| {
        let SyntaxElement::Token(token) = element else {
            return false;
        };
        matches!(
            token.kind(),
            baml_db::baml_compiler_syntax::SyntaxKind::LINE_COMMENT
                | baml_db::baml_compiler_syntax::SyntaxKind::BLOCK_COMMENT
        ) && comment_contains_ignore_directive(token.text())
    })
}

fn comment_contains_ignore_directive(comment: &str) -> bool {
    const PREFIX: &str = "baml-format";
    let lowercase = comment.to_ascii_lowercase();
    lowercase.match_indices(PREFIX).any(|(start, _)| {
        lowercase[start + PREFIX.len()..]
            .trim_start()
            .strip_prefix(':')
            .is_some_and(|rest| rest.trim_start().starts_with("ignore"))
    })
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
            indent_width: CANONICAL_INDENT_WIDTH,
        }
    }
}

/// Canonical indentation used by the CLI, LSP, and default library formatter.
pub const CANONICAL_INDENT_WIDTH: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FormatterError {
    #[error("{0:?}")]
    ParseErrors(Vec<ParseError>),
    #[error("{0}")]
    StrongAstError(#[from] ast::StrongAstError),
}

#[cfg(test)]
mod format_options_tests {
    use super::*;

    #[test]
    fn default_options_use_the_canonical_four_space_indent() {
        let options = FormatOptions::default();
        assert_eq!(CANONICAL_INDENT_WIDTH, 4);
        assert_eq!(options.indent_width, CANONICAL_INDENT_WIDTH);
        let formatted = format("function value() -> int {\n  result\n}\n", &options)
            .expect("default formatter options should format a function body");
        assert_eq!(formatted, "function value() -> int {\n    result\n}\n");
    }
}

#[cfg(test)]
mod redundant_paren_tests {
    use super::*;

    fn fmt(source: &str) -> String {
        let options = FormatOptions::default();
        let formatted = format(source, &options).expect("source should format");
        let second = format(&formatted, &options).expect("formatter should be idempotent");
        assert_eq!(formatted, second, "formatter should be idempotent");
        formatted
    }

    /// B-1562: a left-nested fully parenthesized `&&` chain used to print as
    /// a staircase, one indent level per redundant paren. The parens peel and
    /// the chain flattens like an unparenthesized one; the mixed-precedence
    /// `(x != null)` clause keeps its clarity parens.
    #[test]
    fn test_left_nested_logical_parens_flatten() {
        let source = concat!(
            "test \"doc\" {\n",
            "    let output_document: string? = \"doc\";\n",
            "    assert.is_true(((((output_document != null) && (output_document ?? \"\").includes(\"Shadow\")) && (output_document ?? \"\").includes(\"Amlodipine\")) && (output_document ?? \"\").includes(\"Semintra\")) && (output_document ?? \"\").includes(\"11/04/2025\"))\n",
            "}\n",
        );
        let formatted = fmt(source);
        assert!(
            formatted.contains(concat!(
                "    assert.is_true(\n",
                "        (output_document != null)\n",
                "            && (output_document ?? \"\").includes(\"Shadow\")\n",
                "            && (output_document ?? \"\").includes(\"Amlodipine\")\n",
                "            && (output_document ?? \"\").includes(\"Semintra\")\n",
                "            && (output_document ?? \"\").includes(\"11/04/2025\"),\n",
                "    )",
            )),
            "chain flattens to one indent level: {formatted}"
        );
        assert!(
            !formatted.contains("(("),
            "no nested paren staircase remains: {formatted}"
        );
    }

    /// The verbatim staircase from the B-1562 screenshot: the old formatter's
    /// own output for a generated medical-document test, six nested parens
    /// deep with backtick strings and a `reflect.class.get_field` binding.
    /// Feeding it back in must collapse to the flat chain.
    #[test]
    fn test_b1562_original_example() {
        let source = concat!(
            "class MedicalDoc {\n",
            "    document: string?,\n",
            "    follow_up: string?,\n",
            "}\n",
            "\n",
            "test \"medical_doc\" {\n",
            "    let result = MedicalDoc { document: `Shadow`, follow_up: null };\n",
            "    let output_document = reflect.class.get_field<string?>(result, \"document\");\n",
            "    let output_follow_up = reflect.class.get_field<string?>(result, \"follow_up\");\n",
            "\n",
            "    assert.is_true(\n",
            "        (\n",
            "            (\n",
            "                (\n",
            "                    (\n",
            "                        (\n",
            "                            (\n",
            "                                (output_document != null)\n",
            "                                    && (output_document ?? \"\").includes(`Shadow`)\n",
            "                            )\n",
            "                                && (output_document ?? \"\").includes(`Amlodipine`)\n",
            "                        )\n",
            "                            && (output_document ?? \"\").includes(`Semintra`)\n",
            "                    )\n",
            "                        && (output_document ?? \"\").includes(`BUN`)\n",
            "                )\n",
            "                    && (output_document ?? \"\").includes(`42 mg/dL`)\n",
            "            )\n",
            "                && (output_document ?? \"\").includes(`11/04/2025`)\n",
            "        ),\n",
            "    )\n",
            "}\n",
        );
        let formatted = fmt(source);
        assert!(
            formatted.contains(concat!(
                "    assert.is_true(\n",
                "        (output_document != null)\n",
                "            && (output_document ?? \"\").includes(`Shadow`)\n",
                "            && (output_document ?? \"\").includes(`Amlodipine`)\n",
                "            && (output_document ?? \"\").includes(`Semintra`)\n",
                "            && (output_document ?? \"\").includes(`BUN`)\n",
                "            && (output_document ?? \"\").includes(`42 mg/dL`)\n",
                "            && (output_document ?? \"\").includes(`11/04/2025`),\n",
                "    )",
            )),
            "ticket staircase collapses to a flat chain: {formatted}"
        );
    }

    #[test]
    fn test_same_row_left_parens_strip_single_line() {
        let formatted = fmt("function f(a: int, b: int, c: int) -> int {\n    ((a + b)) - c\n}\n");
        assert!(formatted.contains("    a + b - c\n"), "{formatted}");
        let formatted =
            fmt("function f(a: bool, b: bool, c: bool) -> bool {\n    (a && b) && c\n}\n");
        assert!(formatted.contains("    a && b && c\n"), "{formatted}");
    }

    /// Mixed-precedence parens are redundant to the parser but carry clarity
    /// for the reader; they stay.
    #[test]
    fn test_clarity_parens_are_kept() {
        for expr in ["(a * b) + c", "(a && b) || c", "(a != null) && b"] {
            let source =
                std::format!("function f(a: bool, b: bool, c: bool) -> bool {{\n    {expr}\n}}\n");
            let formatted = fmt(&source);
            assert!(formatted.contains(expr), "kept `{expr}`: {formatted}");
        }
    }

    /// Right-operand parens re-associate if removed; they always stay.
    #[test]
    fn test_right_operand_parens_are_kept() {
        for expr in ["a - (b - c)", "a && (b && c)"] {
            let source =
                std::format!("function f(a: int, b: int, c: int) -> int {{\n    {expr}\n}}\n");
            let formatted = fmt(&source);
            assert!(formatted.contains(expr), "kept `{expr}`: {formatted}");
        }
    }

    /// A transparent paren wrapping a whole call argument carries nothing:
    /// the call's own parens already delimit it.
    #[test]
    fn test_call_argument_parens_strip() {
        let formatted =
            fmt("function f(x: bool) -> null {\n    assert.is_true((x));\n    null\n}\n");
        assert!(formatted.contains("assert.is_true(x);"), "{formatted}");
        let formatted =
            fmt("function f(x: bool) -> null {\n    assert.is_true(((x && x)));\n    null\n}\n");
        assert!(formatted.contains("assert.is_true(x && x);"), "{formatted}");
    }

    /// Parens with a comment on their boundary are not transparent; peeling
    /// them would drop or move the comment, so they stay.
    #[test]
    fn test_comment_bearing_parens_are_kept() {
        let formatted = fmt(
            "function f(a: bool, b: bool, c: bool) -> bool {\n    (a && b /* keep */) && c\n}\n",
        );
        assert!(formatted.contains("(a && b/* keep */) && c"), "{formatted}");
    }

    /// B-1562 follow-up: parens wrapping a *receiver* in a postfix chain.
    /// `(xs).join(x)` and `((xs).join(x)).includes(y)` are pure noise — the
    /// receiver already binds tighter than `.`. Each one used to terminate
    /// the chain walk in `PrintChain::new`, producing one indent level per
    /// paren.
    #[test]
    fn test_postfix_receiver_parens_strip() {
        let formatted =
            fmt("function f(xs: string[]) -> bool {\n    ((xs).join(` `)).includes(`a`)\n}\n");
        assert!(
            formatted.contains("    xs.join(` `).includes(`a`)\n"),
            "{formatted}"
        );
        let formatted =
            fmt("function f(xs: string[]) -> string {\n    (xs.at(0)).to_string()\n}\n");
        assert!(
            formatted.contains("    xs.at(0).to_string()\n"),
            "{formatted}"
        );
        let formatted = fmt("function f(xs: string[]) -> int {\n    (xs).length()\n}\n");
        assert!(formatted.contains("    xs.length()\n"), "{formatted}");
    }

    /// The single-line index path measured and printed the raw base, so
    /// `(xs)[0]` kept its parens inline while the multiline path stripped
    /// them. Optional receivers had the mirror problem: `PrintChain` peeled
    /// them while `single_line_width` still counted the parens, over-measuring
    /// by two per paren and wrapping earlier than needed.
    #[test]
    fn test_index_and_optional_receiver_parens_strip() {
        let formatted = fmt("function f(xs: string[]) -> string {\n    (xs)[0]\n}\n");
        assert!(formatted.contains("    xs[0]\n"), "{formatted}");
        let formatted = fmt("function f(o: string?) -> int? {\n    ((o))?.length\n}\n");
        assert!(formatted.contains("    o?.length\n"), "{formatted}");
        let formatted = fmt("function f(o: string[]?) -> string? {\n    ((o))?.[0]\n}\n");
        assert!(formatted.contains("    o?.[0]\n"), "{formatted}");
        // a looser-binding index receiver still collapses to exactly one paren
        let formatted = fmt("function f(a: string, b: string) -> string {\n    ((a ?? b))[0]\n}\n");
        assert!(formatted.contains("    (a ?? b)[0]\n"), "{formatted}");
    }

    /// Pins the optional-receiver *width* accounting, not just the printed
    /// text: at width 15, `o?.length` (13 cols with indent) fits but the raw
    /// `((o))?.length` (17 cols) does not. If `single_line_width` reverts to
    /// counting the un-peeled base, the expression wraps and this fails even
    /// though the wide-width tests above still pass.
    #[test]
    fn test_optional_receiver_width_counts_effective_base() {
        let options = FormatOptions {
            line_width: 15,
            ..FormatOptions::default()
        };
        let source = "function f(o: string?) -> int? {\n    ((o))?.length\n}\n";
        let formatted = format(source, &options).expect("source should format");
        let second = format(&formatted, &options).expect("formatter should be idempotent");
        assert_eq!(formatted, second, "formatter should be idempotent");
        assert!(formatted.contains("    o?.length\n"), "{formatted}");
    }

    /// The literal restriction exists only to stop `(1).to_string()` from
    /// re-lexing its `.` into a float. No `.` follows a unary operand, so
    /// literals peel there — but a literal that *is* a receiver still keeps
    /// its parens.
    #[test]
    fn test_unary_operand_literal_parens_strip() {
        let formatted = fmt("function f() -> int {\n    -((1))\n}\n");
        assert!(formatted.contains("    -1\n"), "{formatted}");
        let formatted = fmt("function f() -> bool {\n    !((true))\n}\n");
        assert!(formatted.contains("    !true\n"), "{formatted}");
        // the literal here is a postfix receiver, not a unary operand
        let formatted = fmt("function f() -> string {\n    -(1).to_string()\n}\n");
        assert!(formatted.contains("    -(1).to_string()\n"), "{formatted}");
    }

    /// Parens that terminate an optional chain are load-bearing, not
    /// decoration: `(a?.b).c` evaluates `(null).c` — a `TypeError` — when `a` is
    /// null, where `a?.b.c` short-circuits to null. Peeling them would change
    /// runtime behavior, so they always stay, including when the `?.` sits
    /// further down the spine (`(a?.b.c).d`).
    #[test]
    fn test_optional_chain_breaking_parens_are_kept() {
        for expr in [
            "(user?.profile).name",
            "(items?.at(0)).to_string()",
            "(user?.profile.name).length()",
        ] {
            let source = std::format!(
                "function f(user: string?, items: string[]?) -> string {{\n    {expr}\n}}\n"
            );
            let formatted = fmt(&source);
            assert!(formatted.contains(expr), "kept `{expr}`: {formatted}");
        }
    }

    /// A `?.` off the spine — inside a call argument — is a separate chain and
    /// does not pin the receiver's parens.
    #[test]
    fn test_optional_chain_off_the_spine_still_strips() {
        let formatted = fmt(
            "function f(a: string?, xs: string[]) -> int {\n    (xs.at(a?.length ?? 0)).to_string().length()\n}\n",
        );
        assert!(
            formatted.contains("    xs.at(a?.length ?? 0).to_string().length()\n"),
            "{formatted}"
        );
    }

    /// A receiver that binds looser than `.` keeps exactly one paren: removing
    /// it would re-parse against a different base, but the redundant layers
    /// stacked around it still peel.
    #[test]
    fn test_looser_receiver_collapses_to_one_paren() {
        let formatted =
            fmt("function f(a: string, b: string) -> string {\n    ((a ?? b)).to_string()\n}\n");
        assert!(
            formatted.contains("    (a ?? b).to_string()\n"),
            "{formatted}"
        );
        let formatted =
            fmt("function f(a: string, b: string) -> bool {\n    !((a ?? b)).includes(`x`)\n}\n");
        assert!(
            formatted.contains("    !(a ?? b).includes(`x`)\n"),
            "{formatted}"
        );
    }

    /// A receiver that binds looser than `.` keeps its parens: removing them
    /// would re-parse against a different base.
    #[test]
    fn test_postfix_receiver_clarity_parens_are_kept() {
        for expr in ["(a ?? b).length()", "(a && b).to_string()"] {
            let source =
                std::format!("function f(a: string, b: string) -> string {{\n    {expr}\n}}\n");
            let formatted = fmt(&source);
            assert!(formatted.contains(expr), "kept `{expr}`: {formatted}");
        }
    }

    /// A transparent paren around a unary operand that already binds tighter
    /// than the operator carries nothing: `!(x.f())` is `!x.f()`.
    #[test]
    fn test_unary_operand_parens_strip() {
        let formatted =
            fmt("function f(xs: string[]) -> bool {\n    !((xs).join(` `).includes(`a`))\n}\n");
        assert!(
            formatted.contains("    !xs.join(` `).includes(`a`)\n"),
            "{formatted}"
        );
    }

    /// A unary operand that binds looser than the operator keeps its parens.
    #[test]
    fn test_unary_operand_clarity_parens_are_kept() {
        let formatted = fmt("function f(a: bool, b: bool) -> bool {\n    !(a && b)\n}\n");
        assert!(formatted.contains("!(a && b)"), "{formatted}");
    }

    /// The user-reported staircase: a `map`/`join`/`includes` chain nested
    /// under `!` inside a call argument, five parens deep.
    #[test]
    fn test_postfix_receiver_staircase_collapses() {
        let source = concat!(
            "function f(sections: string[], pet_name: string) -> null {\n",
            "    assert.is_true(\n",
            "        (pet_name == `Bella`)\n",
            "            && !(\n",
            "                (\n",
            "                    (\n",
            "                        (sections).map((item) -> {\n",
            "                            item.to_string()\n",
            "                        })\n",
            "                    )\n",
            "                        .join(` `)\n",
            "                )\n",
            "                    .includes(`WarningSignsContact`)\n",
            "            ),\n",
            "    );\n",
            "    null\n",
            "}\n",
        );
        let formatted = fmt(source);
        assert!(
            formatted.contains(concat!(
                "    assert.is_true(\n",
                "        (pet_name == `Bella`)\n",
                "            && !sections\n",
                "                .map((item) -> {\n",
                "                    item.to_string()\n",
                "                })\n",
                "                .join(` `)\n",
                "                .includes(`WarningSignsContact`),\n",
                "    );",
            )),
            "staircase collapses to one flat chain: {formatted}"
        );
    }
}

#[cfg(test)]
mod llm_tools_field_tests {
    use super::*;

    /// The BEP `tools` field must survive formatting — the print path used to
    /// omit it entirely, silently deleting the field (and with it the
    /// function's spec mode) from the user's source.
    #[test]
    fn test_tools_field_is_preserved_and_idempotent() {
        let source = concat!(
            "function Plan(q: string) -> string {\n",
            "    client: \"openai/gpt-4o-mini\"\n",
            "    // the toolbox\n",
            "    tools: [search_flights, search_hotels]\n",
            "    prompt: `\n",
            "        ${q}\n",
            "        ${ctx.output_format}\n",
            "    `\n",
            "}\n",
        );
        let options = FormatOptions::default();
        let formatted = format(source, &options).expect("tools field should format");
        assert!(
            formatted.contains("tools: [search_flights, search_hotels]"),
            "tools field preserved (canonical colon form): {formatted}"
        );
        assert!(
            formatted.contains("// the toolbox"),
            "comment on the tools line preserved: {formatted}"
        );
        let second = format(&formatted, &options).expect("formatter should be idempotent");
        assert_eq!(formatted, second, "formatter should be idempotent");
    }

    #[test]
    fn test_llm_field_comments_before_colons_are_preserved() {
        let source = concat!(
            "function Plan() -> string {\n",
            "    client /* client colon */ : \"openai/gpt-4o-mini\"\n",
            "    tools /* tools colon */ : []\n",
            "    prompt /* prompt colon */ : `hello`\n",
            "}\n",
        );
        let options = FormatOptions::default();
        let formatted = format(source, &options).expect("LLM field comments should format");

        for comment in ["client colon", "tools colon", "prompt colon"] {
            assert!(
                formatted.contains(comment),
                "comment `{comment}` must be preserved: {formatted}"
            );
        }

        let second = format(&formatted, &options).expect("formatter should be idempotent");
        assert_eq!(formatted, second, "formatter should be idempotent");
    }
}

#[cfg(test)]
mod object_spread_tests {
    use super::*;

    /// Struct-update spread (`Type { ...base, field: v }`) compiles, but the
    /// object printer used to reject it outright: "Expected token/node
    /// `OBJECT_FIELD` or `R_BRACE`, but found `SPREAD_ELEMENT`".
    #[test]
    fn test_spread_formats_and_is_idempotent() {
        let source = concat!(
            "class P {\n",
            "    name: string,\n",
            "    score: int,\n",
            "}\n",
            "\n",
            "function f(p: P) -> P {\n",
            "    P { ...p, score: 1 }\n",
            "}\n",
        );
        let options = FormatOptions::default();
        let formatted = format(source, &options).expect("class spread should format");
        assert!(
            formatted.contains("P { ...p, score: 1 }"),
            "spread preserved without a space after `...`: {formatted}"
        );
        let second = format(&formatted, &options).expect("formatter should be idempotent");
        assert_eq!(formatted, second, "formatter should be idempotent");
    }

    /// Member order is semantic — later members win at runtime, so
    /// `P { score: 9, ...p }` and `P { ...p, score: 9 }` evaluate differently.
    /// The formatter must never reorder them, and must keep multiple spreads.
    #[test]
    fn test_spread_and_field_order_is_preserved() {
        let options = FormatOptions::default();
        for (source_expr, expected) in [
            ("P { ...p, score: 9 }", "P { ...p, score: 9 }"),
            ("P { score: 9, ...p }", "P { score: 9, ...p }"),
            ("P { ...a, ...b }", "P { ...a, ...b }"),
            ("P { ...a, score: 1, ...b }", "P { ...a, score: 1, ...b }"),
        ] {
            let source = format!(
                concat!(
                    "class P {{\n",
                    "    name: string,\n",
                    "    score: int,\n",
                    "}}\n",
                    "\n",
                    "function f(p: P, a: P, b: P) -> P {{\n",
                    "    {source_expr}\n",
                    "}}\n",
                ),
                source_expr = source_expr
            );
            let formatted = format(&source, &options)
                .unwrap_or_else(|e| panic!("`{source_expr}` should format: {e:?}"));
            assert!(
                formatted.contains(expected),
                "order preserved for `{source_expr}`, got: {formatted}"
            );
            let second = format(&formatted, &options).expect("formatter should be idempotent");
            assert_eq!(formatted, second, "idempotent for `{source_expr}`");
        }
    }

    /// A spread wide enough to break must survive the multi-line path too,
    /// and comments attached to a spread member must not be dropped.
    #[test]
    fn test_spread_multi_line_and_comments() {
        let source = concat!(
            "class Config {\n",
            "    alpha: string,\n",
            "    beta: string,\n",
            "    gamma: string,\n",
            "}\n",
            "\n",
            "function f(base: Config) -> Config {\n",
            "    Config {\n",
            "        // inherit everything from the base configuration first\n",
            "        ...base,\n",
            "        alpha: \"a much longer override value to force the multi-line path\",\n",
            "        beta: \"another fairly long override value so this cannot fit\",\n",
            "    }\n",
            "}\n",
        );
        let options = FormatOptions::default();
        let formatted = format(source, &options).expect("multi-line spread should format");
        assert!(
            formatted.contains("...base,"),
            "spread member preserved on its own line: {formatted}"
        );
        assert!(
            formatted.contains("// inherit everything from the base configuration first"),
            "comment above the spread preserved: {formatted}"
        );
        let second = format(&formatted, &options).expect("formatter should be idempotent");
        assert_eq!(formatted, second, "formatter should be idempotent");
    }

    /// The spread operand is an arbitrary expression, not just a name.
    #[test]
    fn test_spread_of_call_expression() {
        let source = concat!(
            "class P {\n",
            "    name: string,\n",
            "    score: int,\n",
            "}\n",
            "\n",
            "function base() -> P {\n",
            "    P { name: \"b\", score: 0 }\n",
            "}\n",
            "\n",
            "function f() -> P {\n",
            "    P { ...base(), score: 1 }\n",
            "}\n",
        );
        let options = FormatOptions::default();
        let formatted = format(source, &options).expect("spread of a call should format");
        assert!(
            formatted.contains("P { ...base(), score: 1 }"),
            "call operand preserved: {formatted}"
        );
        let second = format(&formatted, &options).expect("formatter should be idempotent");
        assert_eq!(formatted, second, "formatter should be idempotent");
    }
}

#[cfg(test)]
mod contextual_keyword_identifier_tests {
    use super::*;

    /// `client` lexes as `KW_CLIENT` but is a legal identifier everywhere the
    /// checker accepts one. The formatter must not die on it (it used to:
    /// "Expected token/node of kind WORD, but found `KW_CLIENT`").
    #[test]
    fn test_client_as_identifier_formats() {
        let source = concat!(
            "class Session {\n",
            "    client: string,\n",
            "}\n",
            "\n",
            "function use_it(client: Session, f: (client: Session) -> int) -> int {\n",
            "    let s = Session { client: client.client };\n",
            "    if (s.client == client.client.to_upper_case()) {\n",
            "        return f(client);\n",
            "    }\n",
            "    0\n",
            "}\n",
        );
        let options = FormatOptions::default();
        let formatted = format(source, &options)
            .expect("formatter should accept `client` as field/param/object-key name");
        let second = format(&formatted, &options).expect("formatter should be idempotent");
        assert_eq!(formatted, second, "formatter should be idempotent");
        assert!(formatted.contains("client: string"), "field name preserved");
    }
}

#[cfg(test)]
mod header_comment_position_tests {
    use super::*;

    /// `//#` header comments survive formatting before expression functions and remain structural
    /// at executable statement and arm boundaries.
    #[test]
    fn test_header_comments_in_expression_positions() {
        let source = concat!(
            "//# classify values\n",
            "function classify(n: int) -> string {\n",
            "    //# statements\n",
            "    match (n) {\n",
            "        //# leading header\n",
            "        0 => \"zero\",\n",
            "        //# between arms\n",
            "        _ => \"big\",\n",
            "    }\n",
            "}\n",
        );
        let options = FormatOptions::default();
        let formatted = format(source, &options)
            .expect("formatter should accept header comments in expression positions");
        for needle in [
            "//# classify values",
            "//# statements",
            "//# leading header",
            "//# between arms",
        ] {
            assert!(formatted.contains(needle), "lost {needle}:\n{formatted}");
        }
        let second = format(&formatted, &options).expect("formatter should be idempotent");
        assert_eq!(formatted, second, "formatter should be idempotent");
    }

    #[test]
    fn test_header_comments_in_declarations_are_rejected() {
        for source in [
            "interface Animal {\n    //# methods\n    function name(self) -> string\n}\n",
            "//# generated text\nfunction generate() -> string {\n    client: \"openai/gpt-4o\"\n    prompt: `hello`\n}\n",
        ] {
            let error = format(source, &FormatOptions::default())
                .expect_err("formatter should reject headers outside expression functions");

            assert!(
                format!("{error:?}")
                    .contains("header comments (`//#`) are only allowed in expression functions")
            );
        }
    }
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

    /// A JS/TS-style fat arrow is an unambiguous punctuation slip in a
    /// function signature. The shared parser accepts it and the formatter
    /// repairs it to canonical BAML `->` rather than rejecting the file.
    #[test]
    fn test_top_level_fat_arrow_is_repaired() {
        let source = "function add(a: int, b: int) => int {\n    a + b\n}\n";
        let options = FormatOptions::default();
        let formatted = format(source, &options).expect("formatter should repair a fat arrow");
        assert_eq!(
            formatted,
            "function add(a: int, b: int) -> int {\n    a + b\n}\n"
        );
        let second = format(&formatted, &options).expect("formatter should be idempotent");
        assert_eq!(formatted, second, "formatter should be idempotent");
    }

    #[test]
    fn test_expression_bodied_lambda_is_rejected() {
        let options = FormatOptions::default();
        for arrow in ["->", "=>"] {
            let source = format!(
                "function apply() -> int {{\n    let add_one = (x: int) {arrow} x + 1\n    add_one(41)\n}}\n"
            );
            assert!(
                format(&source, &options).is_err(),
                "formatter must reject a lambda body without braces for {arrow}"
            );
        }
    }

    #[test]
    fn test_arrow_comments_survive_annotated_and_block_bodies() {
        let source = "function top() => /* result */ int { 1 }\n\nfunction apply() -> int {\n    let identity = (x: int) => /* body */ { x }\n    identity(1)\n}\n";
        let options = FormatOptions::default();
        let formatted = format(source, &options).expect("formatter should preserve comments");
        assert!(formatted.contains("-> /* result */ int"));
        assert!(formatted.contains("-> /* body */ {"));
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

    #[test]
    fn test_runtime_type_syntax_formatting_is_idempotent() {
        let source = r#"function f(t: type, value: int) -> int {
    type T = unreflect(t)
    let result = identity<unreflect(t), string>(value)
    match (value) {
        unreflect(t) => result,
        _ => 0
    }
}
"#;
        let options = FormatOptions::default();
        let formatted = format(source, &options).expect("runtime type syntax should format");
        assert!(formatted.contains("type T = unreflect(t)"));
        assert!(formatted.contains("identity<unreflect(t), string>"));
        assert!(formatted.contains("unreflect(t) => result"));
        let second = format(&formatted, &options).expect("formatter should be idempotent");
        assert_eq!(formatted, second);
    }
}

#[cfg(test)]
mod ignore_directive_tests {
    use super::*;

    #[test]
    fn line_comment_directive_preserves_invalid_source_verbatim() {
        let source = "// BAML-FORMAT : ignore\nfunction unfinished(\n";
        let formatted = format(source, &FormatOptions::default())
            .expect("the ignore directive should bypass parse errors");
        assert_eq!(formatted, source);
    }

    #[test]
    fn directive_text_inside_a_string_does_not_disable_formatting() {
        let source = "function main() -> string {\n  let marker = \"// baml-format: ignore\";\n  marker\n}\n";
        let formatted = format(source, &FormatOptions::default())
            .expect("directive-like string content is ordinary source text");
        assert_ne!(formatted, source);
        assert!(formatted.contains("    let marker = \"// baml-format: ignore\";"));
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
        let source = "function Demo(name: string) -> string {\n    client: \"openai/gpt-4o\"\n    prompt: `\n            Hello ${name}\n            Goodbye\n    `\n}\n";
        let expected = "function Demo(name: string) -> string {\n    client: \"openai/gpt-4o\"\n    prompt: `\n        Hello ${name}\n        Goodbye\n    `\n}\n";
        let options = FormatOptions::default();
        let formatted = format(source, &options).expect("formatter should succeed");
        assert_eq!(formatted, expected, "got:\n{formatted}");
        let second = format(&formatted, &options).expect("formatter should be idempotent");
        assert_eq!(formatted, second);
    }

    #[test]
    fn backtick_function_return_dedents() {
        let source = "function Foo(name: string) -> string {\n    `\n            Hello ${name}\n            Bye\n    `\n}\n";
        let expected = "function Foo(name: string) -> string {\n    `\n        Hello ${name}\n        Bye\n    `\n}\n";
        let options = FormatOptions::default();
        let formatted = format(source, &options).expect("formatter should succeed");
        assert_eq!(formatted, expected, "got:\n{formatted}");
        let second = format(&formatted, &options).expect("formatter should be idempotent");
        assert_eq!(formatted, second);
    }

    /// A single-line backtick prompt is accepted and printed verbatim.
    #[test]
    fn backtick_prompt_one_liner_accepted() {
        let source = "function Demo() -> string {\n    client: \"openai/gpt-4o\"\n    prompt: `Just one line`\n}\n";
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
mod contextual_keyword_name_tests {
    //! Regression tests for contextual keywords used as names. The lexer
    //! emits a dedicated keyword kind for `client` (`KW_CLIENT`), and the
    //! parser accepts it as a class field, parameter, and member-access
    //! name (BEP-049 §10 `ctx.client`). The formatter used to reject those
    //! files with "Expected token/node of kind WORD, but found `KW_CLIENT`".

    use super::*;

    fn assert_round_trips(source: &str) {
        let options = FormatOptions::default();
        let formatted = format(source, &options).unwrap_or_else(|error| {
            panic!("formatter should accept contextual keyword name: {error:?}\nsource:\n{source}")
        });
        assert_eq!(formatted, source);
        let second = format(&formatted, &options).expect("formatter should be idempotent");
        assert_eq!(formatted, second, "formatter should be idempotent");
    }

    #[test]
    fn client_as_class_field_name() {
        assert_round_trips("class Agent {\n    client: string?,\n    name: string,\n}\n");
    }

    #[test]
    fn client_as_function_parameter_name() {
        // A `let` statement keeps the body classified as an expression
        // function; a bare `client` tail expression would trip the parser's
        // LLM-function-body heuristic, which is out of the formatter's hands.
        assert_round_trips(
            "function call_llm(client: string, function_name: string) -> string {\n    let out = client;\n    out\n}\n",
        );
    }

    #[test]
    fn client_as_member_access_name() {
        assert_round_trips(
            "class Agent {\n    client: string?,\n}\n\nfunction f(a: Agent) -> string? {\n    let c = a.client;\n    c\n}\n",
        );
    }

    #[test]
    fn client_as_object_literal_key() {
        assert_round_trips(
            "class Agent {\n    client: string?,\n}\n\nfunction make(client: string?) -> Agent {\n    let a = Agent { client: client };\n    a\n}\n",
        );
    }

    #[test]
    fn keyword_method_names_round_trip() {
        // Declaration keywords stay valid as member/path names. Runtime
        // reflection relies on `class`/`enum`/`function` namespace segments,
        // while the
        // reflection API uses `implements` as a method name.
        assert_round_trips(
            "function f(dog_t: type, animal_t: type) -> bool {\n    let views = dog_t.class.enum.function.interface;\n    dog_t.implements(animal_t)\n}\n",
        );
    }
}

#[cfg(test)]
mod spawn_and_hug_format_tests {
    use super::*;

    fn assert_formats_to(source: &str, expected: &str) {
        let options = FormatOptions::default();
        let formatted = format(source, &options).expect("formatter should succeed");
        assert_eq!(formatted, expected);
        let second = format(&formatted, &options).expect("formatter should be idempotent");
        assert_eq!(formatted, second, "formatter should be idempotent");
    }

    /// A relocated `spawn { … }` body reindents like any block instead of
    /// printing verbatim at its original columns, and a call whose sole
    /// argument is a spawn hugs the parens (`push(spawn {` … `});`).
    #[test]
    fn test_spawn_call_arg_hugs_parens() {
        let source = "function f(calls: int[]) -> int[] {\n    let futures: baml.future.Future<int, null>[] = [];\n    for (let c in calls) {\n        futures\n            .push(\n                spawn {\n            compute_something_with(c, \"a long string to push this past the line width limit!\")\n        },\n            );\n    }\n    await baml.future.all(futures)\n}\n";
        let expected = "function f(calls: int[]) -> int[] {\n    let futures: baml.future.Future<int, null>[] = [];\n    for (let c in calls) {\n        futures.push(spawn {\n            compute_something_with(c, \"a long string to push this past the line width limit!\")\n        });\n    }\n    await baml.future.all(futures)\n}\n";
        assert_formats_to(source, expected);
    }

    /// A lambda argument hugs the call parens the same way, including with
    /// leading non-block arguments on the call line.
    #[test]
    fn test_trailing_lambda_arg_hugs_parens() {
        let source = "function f(rows: int[]) -> int {\n    rows.reduce(0, (acc: int, x: int) -> {\n        acc + x + 1000000 + 2000000 + 3000000 + 4000000 + 5000000 + 6000000\n    })\n}\n";
        assert_formats_to(source, source);
    }

    /// Width used by a hugged trailing argument includes the chain prefix
    /// exactly once. Double-counting `receiver.method` would wrap parameters
    /// even though this lambda header fits the configured line width.
    #[test]
    fn test_hug_width_accounts_for_chain_prefix_once() {
        let source = "function f() -> int {\n    receiver.method((a: int, b: int) -> {\n        a + b\n    })\n}\n";
        let options = FormatOptions {
            line_width: 45,
            ..FormatOptions::default()
        };
        let formatted = format(source, &options).expect("formatter should succeed");
        assert_eq!(formatted, source);
        let second = format(&formatted, &options).expect("formatter should be idempotent");
        assert_eq!(formatted, second, "formatter should be idempotent");
    }

    /// The hug layout may remove the trailing comma, but comments attached to
    /// the final argument and comma must remain in the formatted output.
    #[test]
    fn test_hug_preserves_trailing_argument_comments() {
        let source = "function f() -> int {\n    consume(0, /* before spawn */ spawn {\n        let x = 1;\n        x\n    } /* after spawn */, /* after comma */)\n}\n";
        let options = FormatOptions::default();
        let formatted = format(source, &options).expect("formatter should succeed");
        for marker in [
            "/* before spawn */",
            "/* after spawn */",
            "/* after comma */",
        ] {
            assert!(
                formatted.contains(marker),
                "hug comment {marker} must survive, got:\n{formatted}"
            );
        }
        let second = format(&formatted, &options).expect("formatter should be idempotent");
        assert_eq!(formatted, second, "formatter should be idempotent");
    }

    /// A simple spawn body (`{ tail }`) stays on one line when it fits.
    #[test]
    fn test_simple_spawn_stays_single_line() {
        let source = "function f() -> int {\n    let nm = \"n\";\n    let a = spawn nm { 7 };\n    let b = spawn with baml.spawn.options(name = nm) { 8 };\n    (await a) + (await b)\n}\n";
        assert_formats_to(source, source);
    }

    /// A spawn body with statements expands into a normal indented block.
    #[test]
    fn test_multi_statement_spawn_body_expands() {
        let source =
            "function f() -> int {\n    let a = spawn { let x = 1; x + 1 };\n    await a\n}\n";
        let expected = "function f() -> int {\n    let a = spawn {\n        let x = 1;\n        x + 1\n    };\n    await a\n}\n";
        assert_formats_to(source, expected);
    }

    /// An empty block with no interior comment collapses to `{}` — most
    /// visibly in match arms (`null => {},`) and empty `if` bodies.
    #[test]
    fn test_empty_blocks_collapse() {
        let source = "function f(p: string?) -> int {\n    match (p) {\n        null => {\n        },\n        let t: string => {\n            log(t);\n        },\n    }\n    if (p == null) {\n    }\n    0\n}\n";
        let expected = "function f(p: string?) -> int {\n    match (p) {\n        null => {},\n        let t: string => {\n            log(t);\n        },\n    }\n    if (p == null) {}\n    0\n}\n";
        assert_formats_to(source, expected);
    }

    /// An empty block that holds a comment must NOT collapse — the comment
    /// would be lost.
    #[test]
    fn test_empty_block_with_comment_stays_multi_line() {
        let source = "function f(p: string?) -> int {\n    if (p == null) {\n        // nothing to do\n    }\n    0\n}\n";
        let options = FormatOptions::default();
        let formatted = format(source, &options).expect("formatter should succeed");
        assert!(
            formatted.contains("// nothing to do"),
            "interior comment must survive, got:\n{formatted}"
        );
        let second = format(&formatted, &options).expect("formatter should be idempotent");
        assert_eq!(formatted, second, "formatter should be idempotent");
    }

    #[test]
    fn test_spawn_header_comments_are_preserved() {
        let source = "function f(name: string) -> int {\n    let future = spawn // after spawn\n        name /* before with */ with /* before first */ first(), /* after comma */ second() /* before body */ { 1 };\n    await future\n}\n";
        let options = FormatOptions::default();
        let formatted = format(source, &options).expect("formatter should succeed");
        for marker in [
            "// after spawn",
            "/* before with */",
            "/* before first */",
            "/* after comma */",
            "/* before body */",
        ] {
            assert!(
                formatted.contains(marker),
                "spawn header comment {marker} must survive, got:\n{formatted}"
            );
        }
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
mod match_arm_jump_format_tests {
    //! B-619: a braceless `break`/`continue` match arm is wrapped into a block
    //! with a trailing `;` — the same treatment `return` gets — so the output
    //! round-trips through `BREAK_STMT`/`CONTINUE_STMT` and is idempotent.

    use super::*;

    #[test]
    fn braceless_break_and_continue_arms_wrap_into_blocks() {
        let source = r#"function f(n: int) -> int {
  let x = n;
  while (true) {
    match (x) {
      0 => break,
      1 => continue,
      _ => { x = x - 1; }
    }
  }
  x
}
"#;
        let expected = r#"function f(n: int) -> int {
    let x = n;
    while (true) {
        match (x) {
            0 => {
                break;
            },
            1 => {
                continue;
            },
            _ => {
                x = x - 1;
            },
        }
    }
    x
}
"#;
        let options = FormatOptions::default();
        let formatted =
            format(source, &options).expect("formatter should succeed on break/continue arms");
        assert_eq!(
            formatted, expected,
            "formatter output didn't match expected\n--- got ---\n{formatted}\n--- want ---\n{expected}"
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
    fn test_property_shorthand_is_preserved() {
        let source = "function f(options: string) -> map<string, string> {\n    { options, explicit: options }\n}\n";
        assert_formats_to(source, source);
    }

    #[test]
    fn test_object_property_shorthand_is_preserved() {
        let source = "class Config {\n    options: string,\n}\n\nfunction f(options: string) -> Config {\n    Config { options }\n}\n";
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

#[cfg(test)]
mod defer_comment_tests {
    //! Regression tests for B-629: `baml fmt` silently deleted the trailing
    //! line-comment on a `defer { … }` statement while an identical comment on a
    //! normal statement survived. A `defer` statement is an unmodeled node held
    //! as [`crate::ast::Expression::Unknown`] and printed verbatim; it used to
    //! report the whole node span as its leftmost/rightmost token, but the trivia
    //! classifier keys comments to individual *token* ranges, so the comment
    //! attached to the closing `}` token never matched and was dropped. The fix
    //! anchors the node's leading/trailing trivia to its true first/last tokens.

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

    /// The headline case from the ticket: the trailing comment on a `defer`
    /// statement must survive, exactly like the one on the normal statement.
    #[test]
    fn test_defer_trailing_comment_preserved() {
        let source = "function f() -> string[] {\n  let out: string[] = [];\n  defer { out.push(\"x\") }   // THIS comment on a defer line\n  out.push(\"body\");         // this one on a normal line\n  out\n}\n";
        let expected = "function f() -> string[] {\n    let out: string[] = [];\n    defer { out.push(\"x\") } // THIS comment on a defer line\n    out.push(\"body\"); // this one on a normal line\n    out\n}\n";
        assert_formats_to(source, expected);
    }

    /// A trailing comment on a `defer` that is the last statement in the block
    /// (no following statement to "absorb" it) is preserved too.
    #[test]
    fn test_defer_trailing_comment_last_statement_preserved() {
        let source = "function f() -> int {\n    defer { cleanup() } // bye\n    42\n}\n";
        assert_formats_to(source, source);
    }

    /// A leading comment on its own line before a `defer` is preserved and
    /// re-indented to the block indent (it used to be printed verbatim as part
    /// of the node span, keeping its original indentation).
    #[test]
    fn test_defer_leading_comment_reindented() {
        let source = "function f() -> int {\n  // set up cleanup\n  defer { cleanup() }\n  42\n}\n";
        let expected =
            "function f() -> int {\n    // set up cleanup\n    defer { cleanup() }\n    42\n}\n";
        assert_formats_to(source, expected);
    }
}

#[cfg(test)]
mod return_comment_tests {
    //! Regression tests for the same comment-loss class as B-629, applied to a
    //! braceless `return` arm. `return …` in expression position is an unmodeled
    //! node printed verbatim ([`crate::ast::Expression::Return`]); it used to
    //! report the whole node span as its rightmost token. When such an arm has no
    //! trailing comma, the arm's rightmost token delegates to the `return` body,
    //! so a trailing comment (anchored to the return value's last token) never
    //! matched and was silently dropped. Anchoring `Return` to its true first/last
    //! tokens — like the `Unknown` fix — preserves the comment.

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

    /// A trailing comment on a braceless `return` arm with no trailing comma must
    /// survive. The formatter wraps the arm into `{ return …; }` (its established
    /// behavior) and keeps the comment at the arm level.
    #[test]
    fn test_braceless_return_arm_trailing_comment_no_comma_preserved() {
        let source = "function f(x: int) -> int {\n    match (x) {\n        0 => 1,\n        _ => return 2 // fallback\n    }\n}\n";
        let expected = "function f(x: int) -> int {\n    match (x) {\n        0 => 1,\n        _ => {\n            return 2;\n        }, // fallback\n    }\n}\n";
        assert_formats_to(source, expected);
    }

    /// Same, but the arm already has a trailing comma. This case never dropped
    /// the comment, but pins that the wrap + comment placement stays consistent
    /// with the no-comma case.
    #[test]
    fn test_braceless_return_arm_trailing_comment_with_comma_preserved() {
        let source = "function f(x: int) -> int {\n    match (x) {\n        0 => 1,\n        _ => return 2, // fallback\n    }\n}\n";
        let expected = "function f(x: int) -> int {\n    match (x) {\n        0 => 1,\n        _ => {\n            return 2;\n        }, // fallback\n    }\n}\n";
        assert_formats_to(source, expected);
    }

    /// A braceless `return` arm in a `catch` (no comment) still round-trips —
    /// guards the shared wrap helper against regressing the existing behavior.
    #[test]
    fn test_braceless_return_catch_arm_no_comment_round_trips() {
        let source = "function f(x: int) -> int {\n    let v = g(x) catch (e) {\n        _ => return -1,\n    };\n    v\n}\n";
        let expected = "function f(x: int) -> int {\n    let v = g(x) catch (e) {\n        _ => {\n            return -1;\n        },\n    };\n    v\n}\n";
        assert_formats_to(source, expected);
    }
}

#[cfg(test)]
mod member_chain_layout_tests {
    //! Regression tests for member-chain layout. `baml fmt` used to explode
    //! dotted namespace paths one segment per line (`root\n.ai\n.Agent<T>\n…`)
    //! whenever the full expression overflowed the line width. The rule is now
    //! the standard prettier/rustfmt member-chain rule: plain accesses
    //! (namespace segments, field accesses, generic type segments) are atomic
    //! with their receiver, and the chain breaks only at method-call
    //! boundaries — and only when the line overflows.

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

    /// The headline case: the namespace path and both calls stay glued on the
    /// receiver line; only the final call's arguments wrap.
    #[test]
    fn test_namespace_chain_stays_glued_only_args_wrap() {
        let source = "function f() -> int {\n    let result = root.ai.Agent<Itinerary>.new().run(plan_trip_spec(\"plan a weekend trip to yosemite with plenty of hiking\", root.anthropic.AnthropicClient.new()));\n    result\n}\n";
        let expected = "function f() -> int {\n    let result = root.ai.Agent<Itinerary>.new().run(\n        plan_trip_spec(\n            \"plan a weekend trip to yosemite with plenty of hiking\",\n            root.anthropic.AnthropicClient.new(),\n        ),\n    );\n    result\n}\n";
        assert_formats_to(source, expected);
    }

    /// An already-exploded chain (the old formatter's output) collapses back
    /// to the glued layout.
    #[test]
    fn test_exploded_chain_collapses() {
        let source = "function f() -> int {\n    let result = root\n        .ai\n        .Agent<Itinerary>\n        .new()\n        .run(\n            plan_trip_spec(\n                \"plan a weekend trip to yosemite with plenty of hiking\",\n                root.anthropic.AnthropicClient.new(),\n            ),\n        );\n    result\n}\n";
        let expected = "function f() -> int {\n    let result = root.ai.Agent<Itinerary>.new().run(\n        plan_trip_spec(\n            \"plan a weekend trip to yosemite with plenty of hiking\",\n            root.anthropic.AnthropicClient.new(),\n        ),\n    );\n    result\n}\n";
        assert_formats_to(source, expected);
    }

    /// A single call at the end of a namespace path keeps the whole path and
    /// the call glued; the long array argument wraps inside the parens.
    #[test]
    fn test_namespace_call_with_long_array_arg() {
        let source = "function f() -> int {\n    let c = root.ai.ScriptedClient.new([\"the first canned response text\", \"the second canned response text\", \"the third canned response text\"]);\n    c\n}\n";
        let expected = "function f() -> int {\n    let c = root.ai.ScriptedClient.new(\n        [\n            \"the first canned response text\",\n            \"the second canned response text\",\n            \"the third canned response text\",\n        ],\n    );\n    c\n}\n";
        assert_formats_to(source, expected);
    }

    /// A chain too long to keep every call on the receiver line breaks at
    /// method-call boundaries. The namespace path never breaks, and the first
    /// call (`.new()`) stays attached to the path because it fits.
    #[test]
    fn test_long_chain_breaks_at_calls_only() {
        let source = "function f() -> int {\n    let result = root.ai.Agent<Itinerary>.new().with_client(root.anthropic.AnthropicClient.new()).with_options(the_default_options).run(the_spec_value);\n    result\n}\n";
        let expected = "function f() -> int {\n    let result = root.ai.Agent<Itinerary>.new()\n        .with_client(root.anthropic.AnthropicClient.new())\n        .with_options(the_default_options)\n        .run(the_spec_value);\n    result\n}\n";
        assert_formats_to(source, expected);
    }

    /// A dotted path of plain accesses never breaks internally, even past the
    /// line width.
    #[test]
    fn test_plain_access_path_is_atomic() {
        let source = "function f() -> int {\n    let value = root.some_namespace.another_namespace.deeply.nested.module.SomeVeryLongTypeName.CONSTANT_VALUE;\n    value\n}\n";
        assert_formats_to(source, source);
    }

    /// Enum-member paths in match arms (patterns and arm values) stay on one
    /// line.
    #[test]
    fn test_enum_member_paths_in_match_arms() {
        let source = "function f(r: StopReason) -> int {\n    match (r) {\n        StopReason.Complete => 1,\n        StopReason.MaxTokens => 2,\n        _ => root.some.namespaced.StopReason.Complete.value(),\n    }\n}\n";
        assert_formats_to(source, source);
    }

    /// A short chain that fits stays on one line.
    #[test]
    fn test_short_chain_stays_single_line() {
        let source = "function f() -> int {\n    let result = root.ai.Agent<Itinerary>.new().run(spec);\n    result\n}\n";
        assert_formats_to(source, source);
    }

    /// Plain accesses trailing a broken call group stay glued to each other
    /// on the group's line.
    #[test]
    fn test_trailing_plain_accesses_stay_glued() {
        let source = "function f() -> int {\n    let x = builder.configure(a_pretty_long_argument_name, another_pretty_long_argument_name).result.field.value;\n    x\n}\n";
        let expected = "function f() -> int {\n    let x = builder.configure(a_pretty_long_argument_name, another_pretty_long_argument_name)\n        .result.field.value;\n    x\n}\n";
        assert_formats_to(source, expected);
    }

    /// A long chain ending in an optional call (`?.(…)`) uses the tail-broken
    /// layout: the path stays glued, `?.` stays attached to the opening paren,
    /// and only the arguments wrap.
    #[test]
    fn test_optional_call_tail_breaks_args_only() {
        let source = "function f() -> int {\n    let result = root.ai.handlers.maybe_factory?.(the_first_long_argument_name, the_second_long_argument_name, the_third_long_argument_name);\n    result\n}\n";
        let expected = "function f() -> int {\n    let result = root.ai.handlers.maybe_factory?.(\n        the_first_long_argument_name,\n        the_second_long_argument_name,\n        the_third_long_argument_name,\n    );\n    result\n}\n";
        assert_formats_to(source, expected);
    }

    /// A chain whose final member is a long index `[…]` uses the tail-broken
    /// layout too: the path stays glued and the index expression wraps inside
    /// the brackets.
    #[test]
    fn test_final_long_index_breaks_inside_brackets() {
        let source = "function f() -> int {\n    let x = the_data_table.rows_by_category[compute_the_category_key(the_first_component_value, the_second_component_value)];\n    x\n}\n";
        let expected = "function f() -> int {\n    let x = the_data_table.rows_by_category[\n        compute_the_category_key(the_first_component_value, the_second_component_value)\n    ];\n    x\n}\n";
        assert_formats_to(source, expected);
    }

    /// An optional call applied directly to the receiver (`base?.(x).field`)
    /// cannot break away from it: it stays glued to the receiver's line while
    /// later call groups break normally.
    #[test]
    fn test_leading_optional_call_stays_glued_to_receiver() {
        let source = "function f() -> int {\n    let out = fetch_handler?.(the_request_value).response.payload.decode_as_structured(schema_registry_value).validate_against(validation_rules_value);\n    out\n}\n";
        let expected = "function f() -> int {\n    let out = fetch_handler?.(the_request_value).response.payload\n        .decode_as_structured(schema_registry_value)\n        .validate_against(validation_rules_value);\n    out\n}\n";
        assert_formats_to(source, expected);
    }
}
