//! `baml fmt` re-indents multi-line backtick string interiors. Because a
//! backtick string is auto-dedented at lower time (BEP-049 §12), re-indenting
//! must never change the string's runtime value. This test formats a range of
//! backtick sources and checks that:
//!
//!   * [`Expect::Reindented`] cases: every backtick literal's decoded + §12
//!     dedented value is byte-identical before and after formatting. The
//!     formatter only re-indents these because they have no `${for}`/`${if}`
//!     block tag and no multi-line interpolation, so the literal-`${...}` dedent
//!     here faithfully models the lowered value.
//!   * [`Expect::Verbatim`] cases: the backtick literal is left byte-for-byte
//!     unchanged. Their lowered value depends on §13 whitespace control and
//!     placeholder substitution, which this file deliberately does NOT model, so
//!     we assert the stronger "not touched at all" property instead of a value
//!     comparison that could miss a §13 change.

use baml_db::baml_compiler_syntax::{SyntaxKind, SyntaxNode};
use baml_tests::engine::{TestDbExt, db_with_root};

/// Apply the BEP-049 §12 dedent and then decode escapes, as the compiler does
/// for a backtick literal's value (treating `${...}` as literal text — only
/// valid for literals with no block tag and no multi-line interpolation).
fn backtick_value(node_text: &str) -> String {
    let ticks = node_text.bytes().take_while(|&c| c == b'`').count();
    if ticks == 0 || node_text.len() < ticks * 2 {
        return node_text.to_string();
    }
    let inner = &node_text[ticks..node_text.len() - ticks];
    baml_db::escape::unescape_backtick_string_literal(&baml_db::dedent::dedent_backtick(inner))
}

/// Map `f` over every backtick literal in `src`, in source order.
fn backtick_map(src: &str, f: impl Fn(&str) -> String) -> Vec<String> {
    let mut db = db_with_root(std::path::Path::new("."));
    let file = db.file("main.baml", src);
    let tree: SyntaxNode = baml_db::baml_compiler_parser::syntax_tree(&db, file);
    tree.descendants()
        .filter(|n| n.kind() == SyntaxKind::BACKTICK_STRING_LITERAL)
        .map(|n| f(&n.text().to_string()))
        .collect()
}

enum Expect {
    /// The formatter re-indents the interior; its §12 value must be preserved.
    Reindented,
    /// The formatter must leave the literal byte-for-byte unchanged.
    Verbatim,
}

#[test]
fn formatting_preserves_backtick_values() {
    use Expect::{Reindented, Verbatim};
    let cases = [
        // over-indented prompt value
        (
            Reindented,
            "function Demo(name: string) -> string {\n    client: \"openai/gpt-4o\"\n    prompt: `\n            Hello ${name}\n            Goodbye\n    `\n}\n",
        ),
        // function return value
        (
            Reindented,
            "function Header(title: string) -> string {\n    `\n            # ${title}\n    `\n}\n",
        ),
        // attribute argument
        (
            Reindented,
            "class Foo {\n    bar string @description(`\n        some desc\n        more\n    `)\n}\n",
        ),
        // expression position, over-indented
        (
            Reindented,
            "function Demo() -> string {\n    let x = `\n            line one\n            line two\n    `;\n    x\n}\n",
        ),
        // first line less indented than the rest (relative indent is part of the value)
        (
            Reindented,
            "function D() -> string {\n    let x = `first line\n      second line`;\n    x\n}\n",
        ),
        // ragged interior with a blank line
        (
            Reindented,
            "function D() -> string {\n    `\n  alpha\n      beta\n\n  gamma\n  `\n}\n",
        ),
        // already-canonical multi-line text
        (
            Reindented,
            "function Demo() -> string {\n    `\n        first line\n        second line\n    `\n}\n",
        ),
        // single-line literal (no newline, never re-indented)
        (
            Reindented,
            "function Demo() -> string {\n    `hello ${name} world`\n}\n",
        ),
        // B-1474: a trailing `\n` escape is content and must survive formatting
        (
            Reindented,
            "function Demo(host: string) -> string {\n    `\n        ${host}\\n\n    `\n}\n",
        ),
        // block-control template: §13 whitespace control, must stay verbatim
        (
            Verbatim,
            "function F(xs: string[]) -> string {\n    `${for (let x in xs)}- ${x}\n${endfor}`\n}\n",
        ),
        // multi-line interpolation: placeholdered before §12, must stay verbatim
        (
            Verbatim,
            "function D() -> string {\n    let x = `\n        a ${\n            foo()\n        } b\n    `;\n    x\n}\n",
        ),
    ];
    let options = baml_fmt::FormatOptions::default();
    for (expect, src) in cases {
        let formatted = baml_fmt::format(src, &options)
            .unwrap_or_else(|e| panic!("formatter failed on:\n{src}\n{e:?}"));
        match expect {
            Expect::Reindented => assert_eq!(
                backtick_map(src, backtick_value),
                backtick_map(&formatted, backtick_value),
                "formatting changed a backtick value.\n--- source ---\n{src}\n--- formatted ---\n{formatted}",
            ),
            // Compare the raw literal text: the formatter must not touch it at all.
            Expect::Verbatim => assert_eq!(
                backtick_map(src, str::to_string),
                backtick_map(&formatted, str::to_string),
                "backtick literal must be left verbatim.\n--- source ---\n{src}\n--- formatted ---\n{formatted}",
            ),
        }
        // Idempotency, for good measure.
        let twice = baml_fmt::format(&formatted, &options).expect("idempotent");
        assert_eq!(formatted, twice, "not idempotent for:\n{src}");
    }
}
