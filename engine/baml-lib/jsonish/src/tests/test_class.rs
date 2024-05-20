use super::*;

//
const FOO_FILE: &str = r#"
class Foo {
  hi string[]
}

class Bar {
  foo string
}
"#;

// Usage
test_deserializer!(
    test_foo,
    FOO_FILE,
    r#"{"hi": ["a", "b"]}"#,
    FieldType::Class("Foo".to_string()),
    {"hi": ["a", "b"]}
);

test_deserializer!(
    test_wrapped_objects,
    FOO_FILE,
    r#"{"hi": "a"}"#,
    FieldType::List(FieldType::Class("Foo".to_string()).into()),
    [{"hi": ["a"]}]
);

test_deserializer!(
    test_string_from_obj_and_string,
    FOO_FILE,
    r#"The output is: {"hi": ["a", "b"]}"#,
    FieldType::Class("Foo".to_string()),
    {"hi": ["a", "b"]}
);

test_deserializer!(
    test_string_from_obj_and_string_with_extra_text,
    FOO_FILE,
    r#"This is a test. The output is: {"hi": ["a", "b"]}"#,
    FieldType::Class("Foo".to_string()),
    {"hi": ["a", "b"]}
);

test_deserializer!(
    test_string_from_obj_and_string_with_invalid_extra_text,
    FOO_FILE,
    r#"{"hi": ["a", "b"]} is the output."#,
    FieldType::Class("Foo".to_string()),
    {"hi": ["a", "b"]}
);

test_deserializer!(
  str_with_quotes,
  FOO_FILE,
  r#"{"foo": "[\"bar\"]"}"#,
  FieldType::Class("Bar".to_string()),
  {"foo": "[\"bar\"]"}
);

test_deserializer!(
  str_with_nested_json,
  FOO_FILE,
  r#"{"foo": "{\"foo\": [\"bar\"]}"}"#,
  FieldType::Class("Bar".to_string()),
  {"foo": "{\"foo\": [\"bar\"]}"}
);

test_deserializer!(
    test_obj_from_str_with_string_foo,
    FOO_FILE,
    r#"
{  
  "foo": "Here is how you can build the API call:\n```json\n{\n  \"foo\": {\n    \"world\": [\n      \"bar\"\n    ]\n  }\n}\n```"
}
"#,
    FieldType::Class("Bar".to_string()),
    {"foo": "Here is how you can build the API call:\n```json\n{\n  \"foo\": {\n    \"world\": [\n      \"bar\"\n    ]\n  }\n}\n```"}
);

const OPTIONAL_FOO: &str = r#"
class Foo {
  foo string?
}
"#;

test_deserializer!(
    test_optional_foo,
    OPTIONAL_FOO,
    r#"{}"#,
    FieldType::Class("Foo".to_string()),
    { "foo": null }
);

test_deserializer!(
    test_optional_foo_with_value,
    OPTIONAL_FOO,
    r#"{"foo": ""}"#,
    FieldType::Class("Foo".to_string()),
    { "foo": "" }
);

const MULTI_FIELDED_FOO: &str = r#"
class Foo {
  one string
  two string?
}
"#;

test_deserializer!(
    test_multi_fielded_foo,
    MULTI_FIELDED_FOO,
    r#"{"one": "a"}"#,
    FieldType::Class("Foo".to_string()),
    { "one": "a", "two": null }
);

test_deserializer!(
    test_multi_fielded_foo_with_optional,
    MULTI_FIELDED_FOO,
    r#"{"one": "a", "two": "b"}"#,
    FieldType::Class("Foo".to_string()),
    { "one": "a", "two": "b" }
);

test_deserializer!(
    test_multi_fielded_foo_with_optional_and_extra_text,
    MULTI_FIELDED_FOO,
    r#"Here is how you can build the API call:
    ```json
    {
        "one": "hi",
        "two": "hello"
    }
    ```
    
    ```json
        {
            "test2": {
                "key2": "value"
            },
            "test21": [
            ]    
        }
    ```"#,
    FieldType::Class("Foo".to_string()),
    { "one": "hi", "two": "hello" }
);

const MULTI_FIELDED_FOO_WITH_LIST: &str = r#"
class Foo {
  a int
  b string
  c string[]
}
"#;

test_deserializer!(
    test_multi_fielded_foo_with_list,
    MULTI_FIELDED_FOO_WITH_LIST,
    r#"{"a": 1, "b": "hi", "c": ["a", "b"]}"#,
    FieldType::Class("Foo".to_string()),
    { "a": 1, "b": "hi", "c": ["a", "b"] }
);

const NEST_CLASS: &str = r#"
class Foo {
  a string
}

class Bar {
  foo Foo
}
"#;

test_deserializer!(
    test_nested_class,
    NEST_CLASS,
    r#"{"foo": {"a": "hi"}}"#,
    FieldType::Class("Bar".to_string()),
    { "foo": { "a": "hi" } }
);

test_deserializer!(
    test_nested_class_with_extra_text,
    NEST_CLASS,
    r#"Here is how you can build the API call:
    ```json
    {
        "foo": {
            "a": "hi"
        }
    }
    ```
    
    and this
    ```json
    {
        "foo": {
            "a": "twooo"
        }
    }"#,
    FieldType::Class("Bar".to_string()),
    { "foo": { "a": "hi" } }
);

test_deserializer!(
    test_nested_class_with_prefix,
    NEST_CLASS,
    r#"Here is how you can build the API call:
    {
        "foo": {
            "a": "hi"
        }
    }
    
    and this
    {
        "foo": {
            "a": "twooo"
        }
    }
    "#,
    FieldType::Class("Bar".to_string()),
    { "foo": { "a": "hi" } }
);

const NEST_CLASS_WITH_LIST: &str = r#"
class Resume {
    name string?
    email string?
    phone string?
    experience string[] 
    education string[] 
    skills string[] 
}
"#;

test_deserializer!(
    test_resume,
    NEST_CLASS_WITH_LIST,
    r#"{
        "name": "Lee Hsien Loong",
        "email": null,
        "phone": null,
        "experience": [
            "Senior Minister of Singapore since 2024",
            "Prime Minister of Singapore from 2004 to 2024",
            "Member of Parliament (MP) for the Teck Ghee division of Ang Mo Kio GRC since 1991",
            "Teck Ghee SMC between 1984 and 1991",
            "Secretary-General of the People's Action Party (PAP) since 2004"
        ],
        "education": [],
        "skills": ["politician", "former brigadier-general"]
    }"#,
    FieldType::Class("Resume".to_string()),
    {
        "name": "Lee Hsien Loong",
        "email": null,
        "phone": null,
        "experience": [
            "Senior Minister of Singapore since 2024",
            "Prime Minister of Singapore from 2004 to 2024",
            "Member of Parliament (MP) for the Teck Ghee division of Ang Mo Kio GRC since 1991",
            "Teck Ghee SMC between 1984 and 1991",
            "Secretary-General of the People's Action Party (PAP) since 2004"
        ],
        "education": [],
        "skills": ["politician", "former brigadier-general"]
    }
);

test_deserializer!(
    test_resume_partial,
    NEST_CLASS_WITH_LIST,
    r#"{
        "name": "Lee Hsien Loong",
        "email": null,
        "phone": null,
        "experience": [
            "Senior Minister of Singapore since 2024",
            "Prime Minister of Singapore from 2004 to "#,
    FieldType::Class("Resume".to_string()),
    {
        "name": "Lee Hsien Loong",
        "email": null,
        "phone": null,
        "experience": [
            "Senior Minister of Singapore since 2024",
            "Prime Minister of Singapore from 2004 to "
        ],
        "education": [],
        "skills": []
    }
);
