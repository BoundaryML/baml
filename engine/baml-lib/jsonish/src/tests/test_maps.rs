use baml_types::{ir_type::UnionConstructor, type_meta::base::TypeMeta, LiteralValue};

use super::*;
use crate::BamlValueWithFlags;

test_deserializer!(
    test_map,
    "",
    r#"{"a": "b"}"#,
    TypeIR::map(TypeIR::string(), TypeIR::string()),
    {"a": "b"}
);

test_deserializer!(
    test_map_with_quotes,
    "",
    r#"{"\"a\"": "\"b\""}"#,
    TypeIR::map(TypeIR::string(), TypeIR::string()),
    {"\"a\"": "\"b\""}
);

test_deserializer!(
    test_map_with_extra_text,
    "",
    r#"{"a": "b"} is the output."#,
    TypeIR::map(TypeIR::string(), TypeIR::string()),
    {"a": "b"}
);

test_deserializer!(
    test_map_with_invalid_extra_text,
    "",
    r#"{a: b} is the output."#,
    TypeIR::map(TypeIR::string(), TypeIR::string()),
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
    TypeIR::map(TypeIR::string(), TypeIR::class("Foo")),
    {"first":{"a": 1, "b": "hello"}, "second":{"a": 2, "b": "world"}}
);

test_deserializer!(
    test_unterminated_map,
    "",
    r#"
{
    "a": "b
"#,
    TypeIR::map(TypeIR::string(), TypeIR::string()),
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
    TypeIR::map(TypeIR::string(), TypeIR::map(TypeIR::string(), TypeIR::optional(TypeIR::string()))),
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
    TypeIR::map(TypeIR::string(), TypeIR::string()),
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
    TypeIR::map(TypeIR::string(), TypeIR::string()),
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
    let target_type = TypeIR::union(vec![
        TypeIR::class("Foo"),
        TypeIR::map(TypeIR::string(), TypeIR::string()),
    ]);
    let llm_output = r#"{"a": 1, "b": "hello"}"#;
    let expected = json!({"a": "1", "b": "hello"});

    let ir = crate::helpers::load_test_ir(file_content);
    let target = crate::helpers::render_output_format(
        &ir,
        &target_type,
        &Default::default(),
        baml_types::StreamingMode::NonStreaming,
    )
    .unwrap();

    let result = from_str(&target, &target_type, llm_output, true);

    assert!(result.is_ok(), "Failed to parse: {result:?}");

    let value = result.unwrap();
    assert!(matches!(value, BamlValueWithFlags::Class(..)));

    log::trace!("Score: {}", value.score());
    let value: BamlValue = value.into();
    log::info!("{value}");
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
    let target_type = TypeIR::union(vec![
        TypeIR::map(TypeIR::string(), TypeIR::string()),
        TypeIR::class("Foo"),
    ]);
    let llm_output = r#"{"a": 1, "b": "hello"}"#;
    let expected = json!({"a": "1", "b": "hello"});

    let ir = crate::helpers::load_test_ir(file_content);
    let target = crate::helpers::render_output_format(
        &ir,
        &target_type,
        &Default::default(),
        baml_types::StreamingMode::NonStreaming,
    )
    .unwrap();

    let result = from_str(&target, &target_type, llm_output, true);

    assert!(result.is_ok(), "Failed to parse: {result:?}");

    let value = result.unwrap();
    assert!(matches!(value, BamlValueWithFlags::Class(..)));

    log::trace!("Score: {}", value.score());
    let value: BamlValue = value.into();
    log::info!("{value}");
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
  TypeIR::map(TypeIR::r#enum("Key"), TypeIR::string()),
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
    TypeIR::map(TypeIR::r#enum("Key"), TypeIR::string()),
  {"A": "one", "B": "two"}
);

test_partial_deserializer_streaming!(
  test_map_with_literal_keys_streaming,
  "",
  r#"{"A": "one", "B": "two"}"#,
  TypeIR::map(TypeIR::union(vec![
    TypeIR::Literal(LiteralValue::String("A".to_string()), TypeMeta::default()),
    TypeIR::Literal(LiteralValue::String("B".to_string()), TypeMeta::default()),
  ]), TypeIR::string()),
  {"A": "one", "B": "two"}
);
