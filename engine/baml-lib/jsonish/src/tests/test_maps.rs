use super::*;

test_deserializer!(
    test_map,
    "",
    r#"{"a": "b"}"#,
    FieldType::map(FieldType::string(), FieldType::string()).into(),
    {"a": "b"}
);

test_deserializer!(
    test_map_with_quotes,
    "",
    r#"{"\"a\"": "\"b\""}"#,
    FieldType::map(FieldType::string(), FieldType::string()).into(),
    {"\"a\"": "\"b\""}
);

test_deserializer!(
    test_map_with_extra_text,
    "",
    r#"{"a": "b"} is the output."#,
    FieldType::map(FieldType::string(), FieldType::string()).into(),
    {"a": "b"}
);

test_deserializer!(
    test_map_with_invalid_extra_text,
    "",
    r#"{a: b} is the output."#,
    FieldType::map(FieldType::string(), FieldType::string()).into(),
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
    FieldType::map(FieldType::string(), FieldType::class("Foo")).into(),
    {"first":{"a": 1, "b": "hello"}, "second":{"a": 2, "b": "world"}}
);
