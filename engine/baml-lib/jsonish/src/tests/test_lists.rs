use super::*;

test_deserializer!(
    test_list,
    "",
    r#"["a", "b"]"#,
    FieldType::List(FieldType::Primitive(TypeValue::String).into()),
    ["a", "b"]
);

test_deserializer!(
    test_list_with_quotes,
    "",
    r#"["\"a\"", "\"b\""]"#,
    FieldType::List(FieldType::Primitive(TypeValue::String).into()),
    ["\"a\"", "\"b\""]
);

test_deserializer!(
    test_list_with_extra_text,
    "",
    r#"["a", "b"] is the output."#,
    FieldType::List(FieldType::Primitive(TypeValue::String).into()),
    ["a", "b"]
);

test_deserializer!(
    test_list_with_invalid_extra_text,
    "",
    r#"[a, b] is the output."#,
    FieldType::List(FieldType::Primitive(TypeValue::String).into()),
    ["a", "b"]
);

test_deserializer!(
    test_list_object_from_string,
    r#"
    class Foo {
        a int
        b string
        }"#,
    r#"[{"a": 1, "b": "hello"}, {"a": 2, "b": "world"}]"#,
    FieldType::List(FieldType::Class("Foo".to_string()).into()),
    [{"a": 1, "b": "hello"}, {"a": 2, "b": "world"}]
);
