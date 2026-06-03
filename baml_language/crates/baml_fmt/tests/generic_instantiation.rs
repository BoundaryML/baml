//! Formatting of generic instantiation expressions (`foo<int>`, and the
//! non-path-base form `(expr)<int>` — e.g. an inline generic lambda).

use baml_fmt::{FormatOptions, format};

fn fmt(src: &str) -> String {
    format(src, &FormatOptions::default()).unwrap_or_else(|e| {
        panic!("formatter must not error on valid syntax: {e:?}\nsource:\n{src}")
    })
}

/// Every accepted form must format without error and be idempotent.
#[test]
fn generic_instantiation_forms_format_and_are_idempotent() {
    let cases = [
        // path / qualified-path bases (carried by PathExpr::generic_args)
        "function f() -> int {\n  let g = foo<int>\n  g(5)\n}\n",
        "function f() -> int {\n  foo<int>(5)\n}\n",
        "function f() -> int {\n  let g = a.b.foo<int, string>\n  5\n}\n",
        // non-path bases (Expression::GenericApply)
        "function f() -> int {\n  let g = (foo)<int>\n  5\n}\n",
        "function f() -> int {\n  let g = (<T>(x: T) -> T { x })<int>\n  g(5)\n}\n",
    ];
    for src in cases {
        let once = fmt(src);
        let twice = fmt(&once);
        assert_eq!(
            once, twice,
            "formatting must be idempotent for:\n{src}\ngot:\n{once}"
        );
    }
}

/// The trailing `<...>` is preserved (not dropped) for a parenthesized base.
#[test]
fn paren_base_keeps_generic_args() {
    let out = fmt("function f() -> int {\n  let g = (foo)<int>\n  5\n}\n");
    assert!(
        out.contains("(foo)<int>"),
        "expected `(foo)<int>` in:\n{out}"
    );
}
