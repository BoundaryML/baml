use crate::{baml_db, baml_tyannotated};

test_deserializer!(
    test_map,
    r#"{"a": "b"}"#,
    baml_tyannotated!(map<string, string>),
    baml_db!{},
    {"a": "b"}
);

test_deserializer!(
    test_map_with_quotes,
    r#"{"\"a\"": "\"b\""}"#,
    baml_tyannotated!(map<string, string>),
    baml_db!{},
    {"\"a\"": "\"b\""}
);

test_deserializer!(
    test_map_with_extra_text,
    r#"{"a": "b"} is the output."#,
    baml_tyannotated!(map<string, string>),
    baml_db!{},
    {"a": "b"}
);

test_deserializer!(
    test_map_with_invalid_extra_text,
    r#"{a: b} is the output."#,
    baml_tyannotated!(map<string, string>),
    baml_db!{},
    {"a": "b"}
);

test_deserializer!(
    test_map_with_object_values,
    r#"{first: {"a": 1, "b": "hello"}, 'second': {"a": 2, "b": "world"}}"#,
    baml_tyannotated!(map<string, Foo>),
    baml_db!{
        class Foo {
            a: int,
            b: string,
        }
    },
    {"first":{"a": 1, "b": "hello"}, "second":{"a": 2, "b": "world"}}
);

test_deserializer!(
    test_unterminated_map,
    r#"
{
    "a": "b
"#,
    baml_tyannotated!(map<string, string>),
    baml_db!{},
    {"a": "b\n"}
);

test_deserializer!(
    test_unterminated_nested_map,
    r#"
{
    "a": {
        "b": "c",
        "d":
"#,
    baml_tyannotated!(map<string, (map<string, (string | null)>)>),
    baml_db!{},
    // NB: we explicitly drop "d" in this scenario, even though the : gives us a signal that it's a key,
    // and we could default to 'null' for the value, because this is reasonable behavior
    {"a": {"b": "c"}}
);

test_deserializer!(
    test_map_with_newlines_in_keys,
    r#"
{
    "a
    ": "b"}
"#,
    baml_tyannotated!(map<string, string>),
    baml_db!{},
    {"a\n    ": "b"}
);

test_deserializer!(
    test_map_key_coercion,
    r#"
{
    5: "b",
    2.17: "e",
    null: "n"
}
"#,
    baml_tyannotated!(map<string, string>),
    baml_db!{},
    {"5": "b", "2.17": "e", "null": "n"}
);

// test_union_of_class_and_map: union([class Foo, map<string,string>]) should prefer class
test_deserializer!(
    test_union_of_class_and_map,
    r#"{"a": 1, "b": "hello"}"#,
    baml_tyannotated!(Foo | map<string, string>),
    baml_db!{
        class Foo {
            a: string,
            b: string,
        }
    },
    {"a": "1", "b": "hello"}
);

// test_union_of_map_and_class: union([map<string,string>, class Foo]) should still prefer class
test_deserializer!(
    test_union_of_map_and_class,
    r#"{"a": 1, "b": "hello"}"#,
    baml_tyannotated!(Foo | map<string, string>),
    baml_db!{
        class Foo {
            a: string,
            b: string,
        }
    },
    {"a": "1", "b": "hello"}
);

test_deserializer!(
    test_map_with_enum_keys,
    r#"{"A": "one", "B": "two"}"#,
    baml_tyannotated!(map<Key, string>),
    baml_db!{ enum Key { A, B } },
    {"A": "one", "B": "two"}
);

test_partial_deserializer!(
    test_map_with_enum_keys_streaming,
    r#"{"A": "one", "B": "two"}"#,
    baml_tyannotated!(map<Key, string>),
    baml_db!{ enum Key { A, B } },
    {"A": "one", "B": "two"}
);

test_partial_deserializer!(
    test_map_with_literal_keys_streaming,
    r#"{"A": "one", "B": "two"}"#,
    baml_tyannotated!(map<("A" | "B"), string>),
    baml_db!{},
    {"A": "one", "B": "two"}
);
