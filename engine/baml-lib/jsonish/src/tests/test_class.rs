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
    name string
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

test_partial_deserializer!(
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

test_partial_deserializer!(
    test_resume_partial_2,
    NEST_CLASS_WITH_LIST,
    r#"{
        "experience": [
            "Senior Minister of Singapore since 2024",
            "Prime Minister of Singapore from 2004 to "#,
    FieldType::Class("Resume".to_string()),
    {
        "name": null,
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

const CLASS_WITH_ALIASES: &str = r#"
class TestClassAlias {
    key string @alias("key-dash")
    key2 string @alias("key21")
    key3 string @alias("key with space")
    key4 string //unaliased
    key5 string @alias("key.with.punctuation/123")
}
"#;

test_deserializer!(
    test_aliases,
    CLASS_WITH_ALIASES,
    r#"{
        "key-dash": "This is a value with a dash",
        "key21": "This is a value for key21",
        "key with space": "This is a value with space",
        "key4": "This is a value for key4",
        "key.with.punctuation/123": "This is a value with punctuation and numbers"
      }"#,
    FieldType::Class("TestClassAlias".to_string()),
    {
        "key": "This is a value with a dash",
        "key2": "This is a value for key21",
        "key3": "This is a value with space",
        "key4": "This is a value for key4",
        "key5": "This is a value with punctuation and numbers"
    }
);

const CLASS_WITH_NESTED_CLASS_LIST: &str = r#"
class Resume {
    name string
    education Education[]
    skills string[]
  }
  
  class Education {
    school string
    degree string
    year int
  }
"#;

test_deserializer!(
    test_class_with_nested_list,
    CLASS_WITH_NESTED_CLASS_LIST,
    r#"{
        "name": "Vaibhav Gupta",
        "education": [
            {
                "school": "FOOO",
                "degree": "FOOO",
                "year": 2015
            },
            {
                "school": "BAAR",
                "degree": "BAAR",
                "year": 2019
            }
        ],
        "skills": [
          "C++",
          "SIMD on custom silicon"
        ]
      }"#,
    FieldType::Class("Resume".to_string()),
    {
        "name": "Vaibhav Gupta",
        "education": [
            {
                "school": "FOOO",
                "degree": "FOOO",
                "year": 2015
            },
            {
                "school": "BAAR",
                "degree": "BAAR",
                "year": 2019
            }
        ],
        "skills": [
            "C++",
            "SIMD on custom silicon"
        ]
    }
);

test_deserializer!(
    test_class_with_nestedd_list_just_list,
    CLASS_WITH_NESTED_CLASS_LIST,
    r#"[
          {
            "school": "FOOO",
            "degree": "FOOO",
            "year": 2015
          },
          {
            "school": "BAAR",
            "degree": "BAAR",
            "year": 2019
          }
        ]
    "#,
    FieldType::list(FieldType::class("Education")),
        [
            {
                "school": "FOOO",
                "degree": "FOOO",
                "year": 2015
            },
            {
                "school": "BAAR",
                "degree": "BAAR",
                "year": 2019
            }
        ]
);

const FUNCTION_FILE: &str = r#"
class Function {
    selected (Function1 | Function2 | Function3)
}

class Function1 {
    function_name string
    radius int
}

class Function2 {
    function_name string
    diameter int
}

class Function3 {
    function_name string
    length int
    breadth int
}
"#;

test_deserializer!(
    test_obj_created_when_not_present,
    FUNCTION_FILE,
    r#"[
        {
          // Calculate the area of a circle based on the radius.
          function_name: 'circle.calculate_area',
          // The radius of the circle.
          radius: 5,
        },
        {
          // Calculate the circumference of a circle based on the diameter.
          function_name: 'circle.calculate_circumference',
          // The diameter of the circle.
          diameter: 10,
        }
      ]"#,
    FieldType::list(FieldType::class("Function")),
    [
        {"selected": {
            "function_name": "circle.calculate_area",
            "radius": 5
        },
    },
        {"selected":
        {
            "function_name": "circle.calculate_circumference",
            "diameter": 10
        }
        }
    ]
);

test_deserializer!(
    test_trailing_comma_with_space_last_field,
    FUNCTION_FILE,
    r#"
    {
      // Calculate the circumference of a circle based on the diameter.
      function_name: 'circle.calculate_circumference',
      // The diameter of the circle. (with a ", ")
      diameter: 10, 
    }
    "#,
    FieldType::class("Function2"),
    {
        "function_name": "circle.calculate_circumference",
        "diameter": 10
    }
);

test_deserializer!(
    test_trailing_comma_with_space_last_field_and_extra_text,
    FUNCTION_FILE,
    r#"
    {
      // Calculate the circumference of a circle based on the diameter.
      function_name: 'circle.calculate_circumference',
      // The diameter of the circle. (with a ", ")
      diameter: 10, 
      Some key: "Some value"
    }
    and this
    "#,
    FieldType::class("Function2"),
    {
        "function_name": "circle.calculate_circumference",
        "diameter": 10
    }
);

test_failing_deserializer!(
    test_nested_obj_from_string_fails_0,
    r#"
    class Foo {
        foo Bar
    }

    class Bar {
        bar string
        option int?
    }
    "#,
    r#"My inner string"#,
    FieldType::Class("Foo".to_string())
);

test_failing_deserializer!(
    test_nested_obj_from_string_fails_1,
    r#"
    class Foo {
        foo Bar
    }

    class Bar {
        bar string
    }
    "#,
    r#"My inner string"#,
    FieldType::Class("Foo".to_string())
);

test_failing_deserializer!(
    test_nested_obj_from_string_fails_2,
    r#"
    class Foo {
        foo string
    }
    "#,
    r#"My inner string"#,
    FieldType::Class("Foo".to_string())
);

test_deserializer!(
    test_nested_obj_from_int,
    r#"
    class Foo {
        foo int
    }
    "#,
    r#"1214"#,
    FieldType::Class("Foo".to_string()),
    { "foo": 1214 }
);

test_deserializer!(
    test_nested_obj_from_float,
    r#"
    class Foo {
        foo float
    }
    "#,
    r#"1214.123"#,
    FieldType::Class("Foo".to_string()),
    { "foo": 1214.123 }
);

test_deserializer!(
    test_nested_obj_from_bool,
    r#"
    class Foo {
        foo bool
    }
    "#,
    r#" true "#,
    FieldType::Class("Foo".to_string()),
    { "foo": true }
);
