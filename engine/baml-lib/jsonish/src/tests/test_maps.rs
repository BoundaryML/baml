use crate::BamlValueWithFlags;
use baml_types::LiteralValue;

use super::*;

test_deserializer!(
    test_map,
    "",
    r#"{"a": "b"}"#,
    FieldType::map(FieldType::string(), FieldType::string()),
    {"a": "b"}
);

test_deserializer!(
    test_map_with_quotes,
    "",
    r#"{"\"a\"": "\"b\""}"#,
    FieldType::map(FieldType::string(), FieldType::string()),
    {"\"a\"": "\"b\""}
);

test_deserializer!(
    test_map_with_extra_text,
    "",
    r#"{"a": "b"} is the output."#,
    FieldType::map(FieldType::string(), FieldType::string()),
    {"a": "b"}
);

test_deserializer!(
    test_map_with_invalid_extra_text,
    "",
    r#"{a: b} is the output."#,
    FieldType::map(FieldType::string(), FieldType::string()),
    {"a": "b"}
);

test_deserializer!(
    test_map_with_object_values,
    r#"
    class Foo {
        a int
        b string
    }"#,
    r#"{first: {"a": 1, "b": "hello"}, 'second': {"a": 2, "b": "world"}}"#,
    FieldType::map(FieldType::string(), FieldType::class("Foo")),
    {"first":{"a": 1, "b": "hello"}, "second":{"a": 2, "b": "world"}}
);

test_deserializer!(
    test_unterminated_map,
    "",
    r#"
{
    "a": "b
"#,
    FieldType::map(FieldType::string(), FieldType::string()),
    {"a": "b\n"}
);

test_deserializer!(
    test_unterminated_nested_map,
    "",
    r#"
{
    "a": {
        "b": "c",
        "d":
"#,
    FieldType::map(FieldType::string(), FieldType::map(FieldType::string(), FieldType::optional(FieldType::string()))),
    // NB: we explicitly drop "d" in this scenario, even though the : gives us a signal that it's a key,
    // and we could default to 'null' for the value, because this is reasonable behavior
    {"a": {"b": "c"}}
);

test_deserializer!(
    test_map_with_newlines_in_keys,
    "",
    r#"
{
    "a
    ": "b"}
"#,
    FieldType::map(FieldType::string(), FieldType::string()),
    {"a\n    ": "b"}
);

test_deserializer!(
    test_map_key_coercion,
    "",
    r#"
{
    5: "b",
    2.17: "e",
    null: "n"
}
"#,
    FieldType::map(FieldType::string(), FieldType::string()),
    {"5": "b", "2.17": "e", "null": "n"}
);

// test_deserializer!(
//     test_map_key_coercion,
//     "",
//     r#"
// {
//     5: "b"
//     2.17: "e"
//     null: "n"
// }
// "#,
//     FieldType::map(FieldType::string(), FieldType::string()).into(),
//     {"5": "b", "2.17": "e", "null": "n"}
// );

#[test_log::test]
fn test_union_of_class_and_map() {
    let file_content = r#"
    class Foo {
        a string
        b string
    }"#;
    let target_type = FieldType::union(vec![
        FieldType::class("Foo"),
        FieldType::map(FieldType::string(), FieldType::string()),
    ]);
    let llm_output = r#"{"a": 1, "b": "hello"}"#;
    let expected = json!({"a": "1", "b": "hello"});

    let ir = crate::helpers::load_test_ir(file_content);
    let target = crate::helpers::render_output_format(&ir, &target_type, &Default::default()).unwrap();

    let result = from_str(&target, &target_type, llm_output, false);

    assert!(result.is_ok(), "Failed to parse: {:?}", result);

    let value = result.unwrap();
    assert!(matches!(value, BamlValueWithFlags::Class(_, _, _)));

    log::trace!("Score: {}", value.score());
    let value: BamlValue = value.into();
    log::info!("{}", value);
    let json_value = json!(value);

    assert_json_diff::assert_json_eq!(json_value, expected);
}

#[test_log::test]
fn test_union_of_map_and_class() {
    let file_content = r#"
    class Foo {
        a string
        b string
    }"#;
    let target_type = FieldType::union(vec![
        FieldType::map(FieldType::string(), FieldType::string()),
        FieldType::class("Foo"),
    ]);
    let llm_output = r#"{"a": 1, "b": "hello"}"#;
    let expected = json!({"a": "1", "b": "hello"});

    let ir = crate::helpers::load_test_ir(file_content);
    let target = crate::helpers::render_output_format(&ir, &target_type, &Default::default()).unwrap();

    let result = from_str(&target, &target_type, llm_output, false);

    assert!(result.is_ok(), "Failed to parse: {:?}", result);

    let value = result.unwrap();
    assert!(matches!(value, BamlValueWithFlags::Class(_, _, _)));

    log::trace!("Score: {}", value.score());
    let value: BamlValue = value.into();
    log::info!("{}", value);
    let json_value = json!(value);

    assert_json_diff::assert_json_eq!(json_value, expected);
}

test_deserializer!(
  test_map_with_enum_keys,
  r#"
  enum Key {
    A
    B
  }
  "#,
  r#"{"A": "one", "B": "two"}"#,
  FieldType::map(FieldType::Enum("Key".to_string()), FieldType::string()),
  {"A": "one", "B": "two"}
);

test_partial_deserializer_streaming!(
  test_map_with_enum_keys_streaming,
  r#"
  enum Key {
    A
    B
  }
  "#,
  r#"{"A": "one", "B": "two"}"#,
  FieldType::map(FieldType::Enum("Key".to_string()), FieldType::string()),
  {"A": "one", "B": "two"}
);

test_partial_deserializer_streaming!(
  test_map_with_literal_keys_streaming,
  "",
  r#"{"A": "one", "B": "two"}"#,
  FieldType::map(FieldType::Union(vec![
    FieldType::Literal(LiteralValue::String("A".to_string())),
    FieldType::Literal(LiteralValue::String("B".to_string())),
  ]), FieldType::string()),
  {"A": "one", "B": "two"}
);
