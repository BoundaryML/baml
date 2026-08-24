use crate::{testing::CursorTest, type_info::type_at};

#[test]
fn client_config_hover_only_documents_the_key_token() {
    let key = CursorTest::new(
        r#"client<llm> Example {
  prov<[CURSOR]ider openai
}"#,
    );
    let key_markdown = type_at(&key.db, key.cursor.file, key.cursor.offset)
        .expect("config key hover info")
        .to_hover_markdown();
    assert!(key_markdown.contains("provider <name>"));

    let value = CursorTest::new(
        r#"client<llm> Example {
  provider open<[CURSOR]ai
}"#,
    );
    let value_markdown = type_at(&value.db, value.cursor.file, value.cursor.offset)
        .map(|info| info.to_hover_markdown());
    assert!(
        value_markdown
            .as_deref()
            .is_none_or(|markdown| !markdown.contains("provider <name>"))
    );
}

#[test]
fn schema_attribute_hover_only_documents_the_attribute_name() {
    let name = CursorTest::new(
        r#"class Example {
  value string @descrip<[CURSOR]tion("Displayed value")
}"#,
    );
    let name_markdown = type_at(&name.db, name.cursor.file, name.cursor.offset)
        .expect("attribute name hover info")
        .to_hover_markdown();
    assert!(name_markdown.contains(r#"@description("text")"#));

    let argument = CursorTest::new(
        r#"class Example {
  value string @description("Displayed <[CURSOR]value")
}"#,
    );
    let argument_markdown = type_at(&argument.db, argument.cursor.file, argument.cursor.offset)
        .expect("string literal hover info")
        .to_hover_markdown();
    assert!(!argument_markdown.contains(r#"@description("text")"#));
}

#[test]
fn builtin_type_hover_uses_stdlib_docstring() {
    let test = CursorTest::new(
        r#"class Example {
  value i<[CURSOR]nt
}"#,
    );
    let markdown = type_at(&test.db, test.cursor.file, test.cursor.offset)
        .expect("builtin type hover info")
        .to_hover_markdown();

    assert!(markdown.contains("A 63-bit signed integer. Range: -2^62 to 2^62-1"));
    assert!(!markdown.contains("with checked arithmetic"));
}

#[test]
fn intrinsic_type_hover_uses_language_topic() {
    let test = CursorTest::new(
        r#"function stop() -> ne<[CURSOR]ver {
  throw "stop"
}"#,
    );
    let markdown = type_at(&test.db, test.cursor.file, test.cursor.offset)
        .expect("intrinsic type hover info")
        .to_hover_markdown();

    assert!(markdown.contains("The bottom type of an expression that never returns normally."));
}

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
        markdown.contains(
            "function search(query: string, max_results?: int, filter?: string | null) -> int"
        ),
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

#[test]
fn class_hover_lists_out_of_body_implements() {
    let test = CursorTest::new(
        r#"
interface Animal {
  function speak(self) -> string
}

class Dog<[CURSOR] {
  name: string
}

implements Animal for Dog {
  function speak(self) -> string { return self.name }
}
"#,
    );

    let markdown = type_at(&test.db, test.cursor.file, test.cursor.offset)
        .expect("hover info")
        .to_hover_markdown();

    assert!(
        markdown.contains("implements Animal {}"),
        "expected class hover to surface out-of-body implements, got:\n{markdown}"
    );
}

#[test]
fn class_hover_shows_associated_type_bindings_in_implements() {
    let test = CursorTest::new(
        r#"
interface Decoder<Input> {
  type Output
  function decode(self, raw: Input) -> Self.Output
}

class IntDecoder<[CURSOR] {
  implements Decoder<string> {
    type Output = int
    function decode(self, raw: string) -> Self.Output { return 1 }
  }
}
"#,
    );

    let markdown = type_at(&test.db, test.cursor.file, test.cursor.offset)
        .expect("hover info")
        .to_hover_markdown();

    assert!(
        markdown.contains("type Output = int"),
        "expected class hover to include associated type bindings, got:\n{markdown}"
    );
}

#[test]
fn class_hover_shows_describe_hint_when_methods_exist() {
    let test = CursorTest::new(
        r#"/// Does foo things.
class Foo<[CURSOR] {
    bar int

    function greet(self) -> string {
        "hi"
    }
}"#,
    );

    let markdown = type_at(&test.db, test.cursor.file, test.cursor.offset)
        .expect("hover info")
        .to_hover_markdown();

    // Class docstring + fields-only shape.
    assert!(
        markdown.contains("/// Does foo things."),
        "expected class docstring in hover:\n{markdown}"
    );
    assert!(
        markdown.contains("bar: int,"),
        "expected field shape in hover:\n{markdown}"
    );
    // A one-line hint pointing at `baml describe` (canonical FQN at root = bare name).
    assert!(
        markdown.contains("Run `baml describe Foo` for methods and details."),
        "expected describe hint when methods exist:\n{markdown}"
    );
    // Hover does NOT inline method signatures.
    assert!(
        !markdown.contains("function greet"),
        "hover must not list method signatures inline:\n{markdown}"
    );
}

#[test]
fn class_hover_omits_hint_without_methods() {
    let test = CursorTest::new(
        r#"class Point<[CURSOR] {
    x int
    y int
}"#,
    );

    let markdown = type_at(&test.db, test.cursor.file, test.cursor.offset)
        .expect("hover info")
        .to_hover_markdown();

    assert!(
        markdown.contains("x: int,") && markdown.contains("y: int,"),
        "expected field shape in hover:\n{markdown}"
    );
    assert!(
        !markdown.contains("Run `baml describe"),
        "no describe hint should appear for a class without methods:\n{markdown}"
    );
}
