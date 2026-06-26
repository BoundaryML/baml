//! `baml fmt` re-indents multi-line backtick string interiors. Because a
//! backtick string is auto-dedented at lower time (BEP-049 §12), re-indenting
//! must never change the string's runtime value. This test formats a range of
//! backtick sources and checks that every backtick literal's decoded + dedented
//! value is byte-identical before and after formatting.

use baml_db::baml_compiler_syntax::{SyntaxKind, SyntaxNode};
use baml_project::ProjectDatabase;

/// Decode escapes and apply the BEP-049 §12 dedent, exactly as the compiler does
/// for a backtick literal's value (treating `${...}` as literal text).
fn backtick_value(node_text: &str) -> String {
    let ticks = node_text.bytes().take_while(|&c| c == b'`').count();
    if ticks == 0 || node_text.len() < ticks * 2 {
        return node_text.to_string();
    }
    let inner = &node_text[ticks..node_text.len() - ticks];
    let decoded = baml_db::escape::unescape_backtick_string_literal(inner);
    if decoded.contains('\n') {
        baml_db::dedent::preprocess_template(&decoded)
    } else {
        decoded
    }
}

/// The decoded value of every backtick literal in `src`, in source order.
fn backtick_values(src: &str) -> Vec<String> {
    let mut db = ProjectDatabase::new();
    let file = db.add_file("main.baml", src);
    let tree: SyntaxNode = baml_db::baml_compiler_parser::syntax_tree(&db, file);
    tree.descendants()
        .filter(|n| n.kind() == SyntaxKind::BACKTICK_STRING_LITERAL)
        .map(|n| backtick_value(&n.text().to_string()))
        .collect()
}

#[test]
fn formatting_preserves_backtick_values() {
    let cases = [
        // over-indented prompt value
        "function Demo(name: string) -> string {\n    client \"openai/gpt-4o\"\n    prompt `\n            Hello ${name}\n            Goodbye\n    `\n}\n",
        // template_string body
        "template_string Header(title: string) `\n        # ${title}\n`\n",
        // attribute argument
        "class Foo {\n    bar string @description(`\n        some desc\n        more\n    `)\n}\n",
        // expression position, over-indented
        "function Demo() -> string {\n    let x = `\n            line one\n            line two\n    `;\n    x\n}\n",
        // first line less indented than the rest (relative indent is part of the value)
        "function D() -> string {\n    let x = `first line\n      second line`;\n    x\n}\n",
        // ragged interior with a blank line
        "function D() -> string {\n    `\n  alpha\n      beta\n\n  gamma\n  `\n}\n",
        // block-control template (must stay verbatim)
        "function F(xs: string[]) -> string {\n    `${for (let x in xs)}- ${x}\n${endfor}`\n}\n",
        // multi-line interpolation (must stay verbatim)
        "function D() -> string {\n    let x = `\n        a ${\n            foo()\n        } b\n    `;\n    x\n}\n",
        // already-canonical multi-line text
        "function Demo() -> string {\n    `\n        first line\n        second line\n    `\n}\n",
        // single-line literals
        "function Demo() -> string {\n    `hello ${name} world`\n}\n",
    ];
    let options = baml_fmt::FormatOptions::default();
    for src in cases {
        let formatted = baml_fmt::format(src, &options)
            .unwrap_or_else(|e| panic!("formatter failed on:\n{src}\n{e:?}"));
        assert_eq!(
            backtick_values(src),
            backtick_values(&formatted),
            "formatting changed a backtick value.\n--- source ---\n{src}\n--- formatted ---\n{formatted}",
        );
        // Idempotency, for good measure.
        let twice = baml_fmt::format(&formatted, &options).expect("idempotent");
        assert_eq!(formatted, twice, "not idempotent for:\n{src}");
    }
}
