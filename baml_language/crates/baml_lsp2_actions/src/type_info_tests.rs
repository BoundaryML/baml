use crate::{testing::CursorTest, type_info::type_at};

#[test]
fn function_hover_uses_resolved_callback_surface() {
    let test = CursorTest::new(
        r#"function <[CURSOR]forward(cb: (x: int) -> int) -> int {
  return cb(1)
}"#,
    );

    let markdown = type_at(&test.db, test.cursor.file, test.cursor.offset)
        .expect("hover info")
        .to_hover_markdown();

    assert!(
        markdown.contains(
            "function forward(cb: (x: int) -> int throws callback) -> int throws callback"
        ),
        "expected resolved callback throws surface, got:\n{markdown}"
    );
    assert!(
        markdown.contains("Forwards whatever callback `cb` throws."),
        "expected callback forwarding note, got:\n{markdown}"
    );
}

#[test]
fn function_hover_omits_top_level_throws_never_when_implicit() {
    let test = CursorTest::new(
        r#"function <[CURSOR]plain(x: int) -> int {
  return x + 1
}"#,
    );

    let markdown = type_at(&test.db, test.cursor.file, test.cursor.offset)
        .expect("hover info")
        .to_hover_markdown();

    assert!(
        markdown.contains("function plain(x: int) -> int\n```"),
        "expected plain function signature without throws, got:\n{markdown}"
    );
    assert!(
        !markdown.contains("throws never"),
        "implicit non-throwing functions should keep omitting top-level throws never, got:\n{markdown}"
    );
}

#[test]
fn function_hover_shows_explicit_throws_surface() {
    let test = CursorTest::new(
        r#"function <[CURSOR]risky() -> int throws string {
  throw "boom"
}"#,
    );

    let markdown = type_at(&test.db, test.cursor.file, test.cursor.offset)
        .expect("hover info")
        .to_hover_markdown();

    assert!(
        markdown.contains("function risky() -> int throws string"),
        "expected explicit throws surface in hover, got:\n{markdown}"
    );
}

#[test]
fn function_hover_shows_defaulted_params_as_optional() {
    let test = CursorTest::new(
        r#"function <[CURSOR]search(query: string, max_results: int = 10, filter: string? = null) -> int {
  return max_results
}"#,
    );

    let markdown = type_at(&test.db, test.cursor.file, test.cursor.offset)
        .expect("hover info")
        .to_hover_markdown();

    assert!(
        markdown
            .contains("function search(query: string, max_results?: int, filter?: string?) -> int"),
        "expected defaulted params to render with optional markers, got:\n{markdown}"
    );
}

#[test]
fn local_function_type_hover_preserves_optional_param_markers() {
    let test = CursorTest::new(
        r#"function combine(x: int, a: int = 10, b: int = 100) -> int {
  return x + a + b
}

function main() -> int {
  let <[CURSOR]f: (x: int, b?: int) -> int = combine
  return f(1, b = 5)
}"#,
    );

    let markdown = type_at(&test.db, test.cursor.file, test.cursor.offset)
        .expect("hover info")
        .to_hover_markdown();

    assert_eq!(
        markdown,
        "```baml\nf: (x: int, b?: int) -> int throws never\n```"
    );
}

#[test]
fn local_var_hover_for_for_loop_binding_uses_iterable_item_type() {
    let test = CursorTest::new(
        r#"function sum() -> int {
  let total = 0
  for (let <[CURSOR]x in [1, 2]) {
    total += x
  }
  return total
}"#,
    );

    let markdown = type_at(&test.db, test.cursor.file, test.cursor.offset)
        .expect("hover info")
        .to_hover_markdown();

    assert_eq!(markdown, "```baml\nx: int\n```");
}
