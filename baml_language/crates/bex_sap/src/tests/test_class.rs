use crate::{baml_db, baml_tyannotated};

use super::*;

// --- Foo: class with string list ---
// class Foo { hi string[] }
// class Bar { foo string }

test_deserializer!(
    test_foo,
    r#"{"hi": ["a", "b"]}"#,
    baml_tyannotated!(Foo),
    baml_db!{
        class Foo {
            hi: [string],
        }
    },
    {"hi": ["a", "b"]}
);

test_deserializer!(
    test_wrapped_objects,
    r#"{"hi": "a"}"#,
    baml_tyannotated!([Foo]),
    baml_db!{
        class Foo {
            hi: [string],
        }
    },
    [{"hi": ["a"]}]
);

test_deserializer!(
    test_string_from_obj_and_string,
    r#"The output is: {"hi": ["a", "b"]}"#,
    baml_tyannotated!(Foo),
    baml_db!{
        class Foo {
            hi: [string],
        }
    },
    {"hi": ["a", "b"]}
);

test_deserializer!(
    test_string_from_obj_and_string_with_extra_text,
    r#"This is a test. The output is: {"hi": ["a", "b"]}"#,
    baml_tyannotated!(Foo),
    baml_db!{
        class Foo {
            hi: [string],
        }
    },
    {"hi": ["a", "b"]}
);

test_deserializer!(
    test_string_from_obj_and_string_with_invalid_extra_text,
    r#"{"hi": ["a", "b"]} is the output."#,
    baml_tyannotated!(Foo),
    baml_db!{
        class Foo {
            hi: [string],
        }
    },
    {"hi": ["a", "b"]}
);

test_deserializer!(
    str_with_quotes,
    r#"{"foo": "[\"bar\"]"}"#,
    baml_tyannotated!(Bar),
    baml_db!{
        class Bar {
            foo: string,
        }
    },
    {"foo": "[\"bar\"]"}
);

test_deserializer!(
    str_with_nested_json,
    r#"{"foo": "{\"foo\": [\"bar\"]}"}"#,
    baml_tyannotated!(Bar),
    baml_db!{
        class Bar {
            foo: string,
        }
    },
    {"foo": "{\"foo\": [\"bar\"]}"}
);

test_deserializer!(
    test_obj_from_str_with_string_foo,
    r#"
{
  "foo": "Here is how you can build the API call:\n```json\n{\n  \"foo\": {\n    \"world\": [\n      \"bar\"\n    ]\n  }\n}\n```"
}
"#,
    baml_tyannotated!(Bar),
    baml_db!{
        class Bar {
            foo: string,
        }
    },
    {"foo": "Here is how you can build the API call:\n```json\n{\n  \"foo\": {\n    \"world\": [\n      \"bar\"\n    ]\n  }\n}\n```"}
);

// --- Optional Foo ---
// class Foo { foo string? }

test_deserializer!(
    test_optional_foo,
    r#"{}"#,
    baml_tyannotated!(Foo),
    baml_db!{
        class Foo {
            foo: (string | null) @class_completed_field_missing(null),
        }
    },
    { "foo": null }
);

test_deserializer!(
    test_optional_foo_with_value,
    r#"{"foo": ""}"#,
    baml_tyannotated!(Foo),
    baml_db!{
        class Foo {
            foo: (string | null) @class_completed_field_missing(null),
        }
    },
    { "foo": "" }
);

// --- Multi-fielded Foo ---
// class Foo { one string, two string? }

test_deserializer!(
    test_multi_fielded_foo,
    r#"{"one": "a"}"#,
    baml_tyannotated!(Foo),
    baml_db!{
        class Foo {
            one: string,
            two: (string | null) @class_completed_field_missing(null),
        }
    },
    { "one": "a", "two": null }
);

test_deserializer!(
    test_multi_fielded_foo_with_optional,
    r#"{"one": "a", "two": "b"}"#,
    baml_tyannotated!(Foo),
    baml_db!{
        class Foo {
            one: string,
            two: (string | null) @class_completed_field_missing(null),
        }
    },
    { "one": "a", "two": "b" }
);

test_deserializer!(
    test_multi_fielded_foo_with_optional_and_extra_text,
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
    baml_tyannotated!(Foo),
    baml_db!{
        class Foo {
            one: string,
            two: (string | null) @class_completed_field_missing(null),
        }
    },
    { "one": "hi", "two": "hello" }
);

// --- Multi-fielded Foo with list ---
// class Foo { a int, b string, c string[] }

test_deserializer!(
    test_multi_fielded_foo_with_list,
    r#"{"a": 1, "b": "hi", "c": ["a", "b"]}"#,
    baml_tyannotated!(Foo),
    baml_db!{
        class Foo {
            a: int,
            b: string,
            c: [string],
        }
    },
    { "a": 1, "b": "hi", "c": ["a", "b"] }
);

// --- Nested class ---
// class Foo { a string }
// class Bar { foo Foo }

test_deserializer!(
    test_nested_class,
    r#"{"foo": {"a": "hi"}}"#,
    baml_tyannotated!(Bar),
    baml_db!{
        class Foo {
            a: string,
        }
        class Bar {
            foo: Foo,
        }
    },
    { "foo": { "a": "hi" } }
);

test_deserializer!(
    test_nested_class_with_extra_text,
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
    baml_tyannotated!(Bar),
    baml_db!{
        class Foo {
            a: string,
        }
        class Bar {
            foo: Foo,
        }
    },
    { "foo": { "a": "hi" } }
);

test_deserializer!(
    test_nested_class_with_prefix,
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
    baml_tyannotated!(Bar),
    baml_db!{
        class Foo {
            a: string,
        }
        class Bar {
            foo: Foo,
        }
    },
    { "foo": { "a": "hi" } }
);

// --- Resume ---
// class Resume { name string, email string?, phone string?, experience string[], education string[], skills string[] }

test_deserializer!(
    test_resume,
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
    baml_tyannotated!(Resume),
    baml_db!{
        class Resume {
            name: string,
            email: (string | null) @class_completed_field_missing(null),
            phone: (string | null) @class_completed_field_missing(null),
            experience: [string],
            education: [string],
            skills: [string],
        }
    },
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
    r#"{
        "name": "Lee Hsien Loong",
        "email": null,
        "phone": null,
        "experience": [
            "Senior Minister of Singapore since 2024",
            "Prime Minister of Singapore from 2004 to "#,
    baml_tyannotated!(Resume),
    baml_db!{
        class Resume {
            name: string @class_in_progress_field_missing(null),
            email: (string | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
            phone: (string | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
            experience: [string] @class_in_progress_field_missing(null),
            education: [string] @class_in_progress_field_missing([]),
            skills: [string] @class_in_progress_field_missing([]),
        }
    },
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
    r#"{
        "experience": [
            "Senior Minister of Singapore since 2024",
            "Prime Minister of Singapore from 2004 to "#,
    baml_tyannotated!(Resume),
    baml_db!{
        class Resume {
            name: string @class_in_progress_field_missing(null),
            email: (string | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
            phone: (string | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
            experience: [string] @class_in_progress_field_missing(null),
            education: [string] @class_in_progress_field_missing([]),
            skills: [string] @class_in_progress_field_missing([]),
        }
    },
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

// --- Class with aliases ---
// class TestClassAlias {
//     key string @alias("key-dash")
//     key2 string @alias("key21")
//     key3 string @alias("key with space")
//     key4 string //unaliased
//     key5 string @alias("key.with.punctuation/123")
// }

test_deserializer!(
    test_aliases,
    r#"{
        "key-dash": "This is a value with a dash",
        "key21": "This is a value for key21",
        "key with space": "This is a value with space",
        "key4": "This is a value for key4",
        "key.with.punctuation/123": "This is a value with punctuation and numbers"
      }"#,
    baml_tyannotated!(TestClassAlias),
    baml_db!{
        class TestClassAlias {
            key: string @alias("key-dash"),
            key2: string @alias("key21"),
            key3: string @alias("key with space"),
            key4: string,
            key5: string @alias("key.with.punctuation/123"),
        }
    },
    {
        "key": "This is a value with a dash",
        "key2": "This is a value for key21",
        "key3": "This is a value with space",
        "key4": "This is a value for key4",
        "key5": "This is a value with punctuation and numbers"
    }
);

// --- Simple class with nested class ---
// class SimpleTest { answer Answer }
// class Answer { content float }

test_deserializer!(
    test_class_with_whitespace_keys,
    r#"{" answer ": {" content ": 78.54}}"#,
    baml_tyannotated!(SimpleTest),
    baml_db!{
        class Answer {
            content: float,
        }
        class SimpleTest {
            answer: Answer,
        }
    },
    {
        "answer": {
            "content": 78.54
        }
    }
);

// --- Class with nested class list ---
// class Resume { name string, education Education[], skills string[] }
// class Education { school string, degree string, year int }

test_deserializer!(
    test_class_with_nested_list,
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
    baml_tyannotated!(Resume),
    baml_db!{
        class Education {
            school: string,
            degree: string,
            year: int,
        }
        class Resume {
            name: string,
            education: [Education],
            skills: [string],
        }
    },
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
    baml_tyannotated!([Education]),
    baml_db!{
        class Education {
            school: string,
            degree: string,
            year: int,
        }
    },
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

// --- Function classes with union ---
// class Function { selected (Function1 | Function2 | Function3) }
// class Function1 { function_name string, radius int }
// class Function2 { function_name string, diameter int }
// class Function3 { function_name string, length int, breadth int }

fn make_function_db() -> TypeRefDb<'static, &'static str> {
    crate::baml_db! {
        class Function1 {
            function_name: string,
            radius: int,
        }
        class Function2 {
            function_name: string,
            diameter: int,
        }
        class Function3 {
            function_name: string,
            length: int,
            breadth: int,
        }
        class Function {
            selected: (Function1 | Function2 | Function3),
        }
    }
}

test_deserializer!(
    test_obj_created_when_not_present,
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
    baml_tyannotated!([Function]),
    make_function_db(),
    [
        {"selected": {
            "function_name": "circle.calculate_area",
            "radius": 5
        }},
        {"selected": {
            "function_name": "circle.calculate_circumference",
            "diameter": 10
        }}
    ]
);

test_deserializer!(
    test_trailing_comma_with_space_last_field,
    r#"
    {
      // Calculate the circumference of a circle based on the diameter.
      function_name: 'circle.calculate_circumference',
      // The diameter of the circle. (with a ", ")
      diameter: 10,
    }
    "#,
    baml_tyannotated!(Function2),
    baml_db!{
        class Function2 {
            function_name: string,
            diameter: int,
        }
    },
    {
        "function_name": "circle.calculate_circumference",
        "diameter": 10
    }
);

test_deserializer!(
    test_trailing_comma_with_space_last_field_and_extra_text,
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
    baml_tyannotated!(Function2),
    baml_db!{
        class Function2 {
            function_name: string,
            diameter: int,
        }
    },
    {
        "function_name": "circle.calculate_circumference",
        "diameter": 10
    }
);

// --- Nested obj from string fails ---
// class Foo { foo Bar }
// class Bar { bar string, option int? }

test_failing_deserializer!(
    test_nested_obj_from_string_fails_0,
    r#"My inner string"#,
    baml_tyannotated!(Foo),
    baml_db! {
        class Bar {
            bar: string,
            option: (int | null) @class_completed_field_missing(null),
        }
        class Foo {
            foo: Bar,
        }
    }
);

test_failing_deserializer!(
    test_nested_obj_from_string_fails_1,
    r#"My inner string"#,
    baml_tyannotated!(Foo),
    baml_db! {
        class Bar {
            bar: string,
        }
        class Foo {
            foo: Bar,
        }
    }
);

test_failing_deserializer!(
    test_nested_obj_from_string_fails_2,
    r#"My inner string"#,
    baml_tyannotated!(Foo),
    baml_db! {
        class Foo {
            foo: string,
        }
    }
);

test_deserializer!(
    test_nested_obj_from_int,
    r#"1214"#,
    baml_tyannotated!(Foo),
    baml_db!{
        class Foo {
            foo: int,
        }
    },
    { "foo": 1214 }
);

test_deserializer!(
    test_nested_obj_from_float,
    r#"1214.123"#,
    baml_tyannotated!(Foo),
    baml_db!{
        class Foo {
            foo: float,
        }
    },
    { "foo": 1214.123 }
);

test_deserializer!(
    test_nested_obj_from_bool,
    r#" true "#,
    baml_tyannotated!(Foo),
    baml_db!{
        class Foo {
            foo: bool,
        }
    },
    { "foo": true }
);

// --- Nested classes with aliases ---
// class Nested { prop3 string|null, prop4 string|null @alias("blah"), prop20 Nested2 }
// class Nested2 { prop11 string|null, prop12 string|null @alias("blah") }
// class Schema { prop1 string|null, prop2 Nested|string, prop5 (string|null)[], prop6 string|Nested[] @alias("blah"), nested_attrs (string|null|Nested)[], parens (string|null), other_group (string|(int|string)) @alias(other) }

test_deserializer!(
    test_nested_classes_with_aliases,
    r#"
```json
{
  "prop1": "one",
  "prop2": {
    "prop3": "three",
    "blah": "four",
    "prop20": {
      "prop11": "three",
      "blah": "four"
    }
  },
  "prop5": ["hi"],
  "blah": "blah",
  "nested_attrs": ["nested"],
  "parens": "parens1",
  "other": "other"
}
```
"#,
    baml_tyannotated!(Schema),
    baml_db!{
        class Nested2 {
            prop11: (string | null) @class_completed_field_missing(null),
            prop12: (string | null) @alias("blah") @class_completed_field_missing(null),
        }
        class Nested {
            prop3: (string | null) @class_completed_field_missing(null),
            prop4: (string | null) @alias("blah") @class_completed_field_missing(null),
            prop20: Nested2,
        }
        class Schema {
            prop1: (string | null) @class_completed_field_missing(null),
            prop2: (Nested | string),
            prop5: [(string | null)],
            prop6: (string | [Nested]) @alias("blah"),
            nested_attrs: [(string | null | Nested)],
            parens: (string | null) @class_completed_field_missing(null),
            other_group: (string | (int | string)) @alias("other"),
        }
    },
    {
        "prop1": "one",
        "prop2": {
          "prop3": "three",
          "prop4": "four",
          "prop20": {
            "prop11": "three",
            "prop12": "four"
          }
        },
        "prop5": ["hi"],
        "prop6": "blah",
        "nested_attrs": ["nested"],
        "parens": "parens1",
        "other_group": "other"
    }
);

// --- Notion Page test (test_ekinsdrow) ---
// This is a large test with many interconnected types for Notion blocks.

test_deserializer!(
    test_ekinsdrow,
    r#"{
  "object": "page",
  "icon": {
    "emoji": "📚"
  },
  "children": [
    {
      "type": "column_list",
      "column_list": {
        "children": [
          {
            "type": "column",
            "column": {
              "children": [
                {
                  "type": "heading_3",
                  "heading_3": {
                    "rich_text": [
                      {
                        "type": "text",
                        "text": {
                          "content": "The Lord of the Rings"
                        }
                      }
                    ],
                    "is_toggleable": false
                  }
                },
                {
                  "type": "paragraph",
                  "paragraph": {
                    "rich_text": [
                      {
                        "type": "text",
                        "text": {
                          "content": "J.R.R. Tolkien"
                        }
                      }
                    ]
                  }
                },
                {
                  "type": "to_do",
                  "to_do": {
                    "rich_text": [
                      {
                        "type": "text",
                        "text": {
                          "content": "Read again"
                        }
                      }
                    ],
                    "checked": false
                  }
                }
              ]
            }
          }
        ]
      }
    }
  ]
}"#,
    baml_tyannotated!(Page),
    baml_db!{
        enum ColumnType {
            Column @alias("column")
        }
        enum BreadcrumbType {
            Breadcrumb @alias("breadcrumb")
        }
        enum ColumnListType {
            ColumnList @alias("column_list")
        }
        enum Heading3Type {
            Heading3 @alias("heading_3")
        }
        enum ParagraphType {
            Paragraph @alias("paragraph")
        }
        enum RichTextType {
            RichText @alias("text")
        }
        enum ToDoType {
            ToDo @alias("to_do")
        }
        class RichTextContent {
            content: string,
        }
        class RichText {
            r#type: RichTextType,
            text: RichTextContent,
        }
        class HeadingBody {
            rich_text: [RichText],
            is_toggleable: bool,
        }
        class Heading3 {
            r#type: Heading3Type,
            heading_3: HeadingBody,
        }
        class ParagraphBody {
            rich_text: [RichText],
            children: [string] @class_in_progress_field_missing([]) @class_completed_field_missing([]),
        }
        class Paragraph {
            r#type: ParagraphType,
            paragraph: ParagraphBody,
        }
        class ToDoBody {
            rich_text: [RichText],
            checked: (bool | null) @class_completed_field_missing(null),
            children: [Paragraph] @class_in_progress_field_missing([]) @class_completed_field_missing([]),
        }
        class ToDo {
            r#type: ToDoType,
            to_do: ToDoBody,
        }
        class Breadcrumb {
            r#type: BreadcrumbType,
            breadcrumb: map<string, string>,
        }
        class Breadcrumb1 {
            r#type: BreadcrumbType,
            breadcrumb: map<string, string>,
        }
        class ColumnBody {
            children: [(Breadcrumb1 | Heading3 | Paragraph | ToDo)],
        }
        class Column {
            r#type: ColumnType,
            column: ColumnBody,
        }
        class ColumnListBody {
            children: [Column],
        }
        class ColumnList {
            r#type: ColumnListType,
            column_list: ColumnListBody,
        }
        class Icon {
            emoji: string,
        }
        class Page {
            object: string,
            icon: Icon,
            children: [(Breadcrumb | ColumnList | Heading3 | Paragraph | ToDo)],
        }
    },
    {
        "object": "page",
        "icon": {
          "emoji": "\u{1F4DA}"
        },
        "children": [
          {
            "type": "ColumnList",
            "column_list": {
              "children": [
                {
                  "type": "Column",
                  "column": {
                    "children": [
                      {
                        "type": "Heading3",
                        "heading_3": {
                          "rich_text": [
                            {
                              "type": "RichText",
                              "text": {
                                "content": "The Lord of the Rings"
                              }
                            }
                          ],
                          "is_toggleable": false
                        }
                      },
                      {
                        "type": "Paragraph",
                        "paragraph": {
                          "rich_text": [
                            {
                              "type": "RichText",
                              "text": {
                                "content": "J.R.R. Tolkien"
                              }
                            }
                          ],
                        "children": []
                        }
                      },
                      {
                        "type": "ToDo",
                        "to_do": {
                          "rich_text": [
                            {
                              "type": "RichText",
                              "text": {
                                "content": "Read again"
                              }
                            }
                          ],
                          "checked": false,
                        "children": []
                        }
                      }
                    ]
                  }
                }
              ]
            }
          }
        ]
    }
);

// --- Escaped quotes test ---
// class DoCommandACReturnType { sections (TextSection | CodeSection)[] }
// class TextSection { text string }
// class CodeSection { code_language string, code string }

test_deserializer!(
    test_escaped_quotes,
    r#"
Certainly! I'll redesign the UI to make it more appealing to a female audience. I'll focus on color schemes, fonts, and imagery that are generally more attractive to women. Here's my thought process and suggestions:

Thoughts: "The current design is quite neutral. We can make it more feminine by using softer colors, curved shapes, and adding some playful elements. We should also consider updating the trending items to be more relevant to a female audience."

"We can use a pastel color scheme, which is often associated with femininity. Let's go with a soft pink as the primary color, with accents of lavender and mint green."

"For the font, we can use a more elegant and rounded typeface for the logo and headings. This will give a softer, more feminine look."

"We should update the trending items to include more fashion-focused and accessory items that are popular among women."

Here's the redesigned code with these changes:

{
  "sections": [
    {
      "code_language": "swift",
      "code": "import SwiftUI\n\nstruct ContentView: View {\n    var body: some View {\n        ZStack(alignment: .bottom) {\n            VStack(spacing: 0) {\n                CustomNavigationBar()\n                \n                ScrollView {\n                    VStack(spacing: 20) {\n                        LogoSection()\n                        TrendingSection()\n                    }\n                    .padding()\n                }\n            }\n            .background(Color(\"SoftPink\")) // Change background to soft pink\n            \n            BottomSearchBar()\n        }\n        .edgesIgnoringSafeArea(.bottom)\n    }\n}\n"
    },
    {
      "text": "To complete this redesign, you'll need to add some custom colors to your asset catalog."
    }
  ]

  "#,
    baml_tyannotated!(DoCommandACReturnType),
    baml_db!{
        class TextSection {
            text: string,
        }
        class CodeSection {
            code_language: string,
            code: string,
        }
        class DoCommandACReturnType {
            sections: [TextSection | CodeSection],
        }
    },
    {
        "sections": [
            {
                "code_language": "swift",
                "code": "import SwiftUI\n\nstruct ContentView: View {\n    var body: some View {\n        ZStack(alignment: .bottom) {\n            VStack(spacing: 0) {\n                CustomNavigationBar()\n                \n                ScrollView {\n                    VStack(spacing: 20) {\n                        LogoSection()\n                        TrendingSection()\n                    }\n                    .padding()\n                }\n            }\n            .background(Color(\"SoftPink\")) // Change background to soft pink\n            \n            BottomSearchBar()\n        }\n        .edgesIgnoringSafeArea(.bottom)\n    }\n}\n"
            },
            {
                "text": "To complete this redesign, you'll need to add some custom colors to your asset catalog."
            }
        ]
    }
);

// --- Object stream test ---
// class Foo { a int, c int, b int }

test_partial_deserializer!(
    test_object_finished_ints,
    r#"{"a": 1234,"b": 1234, "c": 1234}"#,
    baml_tyannotated!(Foo),
    baml_db!{
        class Foo {
            a: int,
            c: int,
            b: int,
        }
    },
    {"a": 1234, "b": 1234, "c": 1234}
);

// --- Empty string value ---

test_deserializer!(
    test_empty_string_value,
    r#"{"a": ""}"#,
    baml_tyannotated!(Foo),
    baml_db!{
        class Foo {
            a: string,
        }
    },
    {"a": ""}
);

test_deserializer!(
    test_empty_string_value_1,
    r#"{a: ""}"#,
    baml_tyannotated!(Foo),
    baml_db!{
        class Foo {
            a: string,
        }
    },
    {"a": ""}
);

test_deserializer!(
    test_empty_string_value_2,
    r#"{
    a: "",
    b: "",
    res: []
  }"#,
    baml_tyannotated!(Foo),
    baml_db!{
        class Foo {
            a: string,
            b: string,
            res: [string],
        }
    },
    {"a": "", "b": "", "res": []}
);

test_deserializer!(
    test_string_field_with_spaces,
    r#"{
    a: Hi friends!,
    b: hey world lets do something kinda cool
    so that we can test this out,
    res: [hello,
     world]
  }"#,
    baml_tyannotated!(Foo),
    baml_db!{
        class Foo {
            a: string,
            b: string,
            res: [string],
        }
    },
    {
        "a": "Hi friends!",
        "b": "hey world lets do something kinda cool\n    so that we can test this out",
        "res": ["hello", "world"]
    }
);

// --- Recursive type ---
// class Foo { pointer Foo? }

test_deserializer!(
    test_recursive_type,
    r#"
    The answer is
    {
      "pointer": {
        "pointer": null
      }
    },

    Anything else I can help with?
  "#,
    baml_tyannotated!(Foo),
    baml_db!{
        class Foo {
            pointer: (Foo | null) @class_completed_field_missing(null),
        }
    },
    {
        "pointer": {
            "pointer": null,
        },
    }
);

test_deserializer!(
    test_recursive_type_missing_brackets_and_quotes,
    r#"
    The answer is
    {
      "pointer": {
        pointer: null,

    Anything else I can help with?
  "#,
    baml_tyannotated!(Foo),
    baml_db!{
        class Foo {
            pointer: (Foo | null) @class_completed_field_missing(null),
        }
    },
    {
        "pointer": {
            "pointer": null,
        },
    }
);

// --- Recursive type with union ---
// class Foo { pointer Foo | int }

test_deserializer!(
    test_recursive_type_with_union,
    r#"
    The answer is
    {
      "pointer": {
        "pointer": 1,
      }
    },

    Anything else I can help with?
  "#,
    baml_tyannotated!(Foo),
    baml_db!{
        class Foo {
            pointer: (Foo | int),
        }
    },
    {
        "pointer": {
            "pointer": 1,
        },
    }
);

// --- Mutually recursive ---
// class Foo { b Bar | int }
// class Bar { f Foo | int }

test_deserializer!(
    test_mutually_recursive_with_union,
    r#"
    The answer is
    {
      "b": {
        "f": {
          "b": 1
        },
      }
    },

    Anything else I can help with?
  "#,
    baml_tyannotated!(Foo),
    baml_db!{
        class Foo {
            b: (Bar | int),
        }
        class Bar {
            f: (Foo | int),
        }
    },
    {
        "b": {
            "f": {
                "b": 1,
            },
        },
    }
);

test_deserializer!(
    test_recursive_type_with_union_missing_brackets_and_quotes,
    r#"
    The answer is
    {
      "pointer": {
        pointer: 1
    },

    Anything else I can help with?
  "#,
    baml_tyannotated!(Foo),
    baml_db!{
        class Foo {
            pointer: (Foo | int),
        }
    },
    {
        "pointer": {
            "pointer": 1,
        },
    }
);

// --- Recursive union on multiple fields ---
// class Foo { rec_one Foo | int, rec_two Foo | int }

test_deserializer!(
    test_recursive_union_on_multiple_fields_single_line,
    r#"
    The answer is
    {
      "rec_one": { "rec_one": 1, "rec_two": 2 },
      "rec_two": {
        "rec_one": { "rec_one": 1, "rec_two": 2 },
        "rec_two": { "rec_one": 1, "rec_two": 2 }
      }
    },

    Anything else I can help with?
  "#,
    baml_tyannotated!(Foo),
    baml_db!{
        class Foo {
            rec_one: (Foo | int),
            rec_two: (Foo | int),
        }
    },
    {
        "rec_one": {
            "rec_one": 1,
            "rec_two": 2
        },
        "rec_two": {
            "rec_one": {
                "rec_one": 1,
                "rec_two": 2
            },
            "rec_two": {
                "rec_one": 1,
                "rec_two": 2
            }
        },
    }
);

test_deserializer!(
    test_recursive_union_on_multiple_fields_single_line_without_quotes,
    r#"
    The answer is
    {
      rec_one: { rec_one: 1, rec_two: 2 },
      rec_two: {
        rec_one: { rec_one: 1, rec_two: 2 },
        rec_two: { rec_one: 1, rec_two: 2 }
      }
    },

    Anything else I can help with?
  "#,
    baml_tyannotated!(Foo),
    baml_db!{
        class Foo {
            rec_one: (Foo | int),
            rec_two: (Foo | int),
        }
    },
    {
        "rec_one": {
            "rec_one": 1,
            "rec_two": 2
        },
        "rec_two": {
            "rec_one": {
                "rec_one": 1,
                "rec_two": 2
            },
            "rec_two": {
                "rec_one": 1,
                "rec_two": 2
            }
        },
    }
);

// --- Recursive single line with bool ---
// class Foo { rec_one Foo | int | bool, rec_two Foo | int | bool }

test_deserializer!(
    test_recursive_single_line,
    r#"
    The answer is
    { rec_one: true, rec_two: false },

    Anything else I can help with?
  "#,
    baml_tyannotated!(Foo),
    baml_db!{
        class Foo {
            rec_one: (Foo | int | bool),
            rec_two: (Foo | int | bool),
        }
    },
    {
        "rec_one": true,
        "rec_two": false
    }
);

// --- Complex recursive union ---
// class Foo { rec_one Foo | int | bool, rec_two Foo | int | bool | null }

test_deserializer!(
    test_recursive_union_on_multiple_fields_single_line_without_quotes_complex,
    r#"
    The answer is
    {
      rec_one: { rec_one: { rec_one: true, rec_two: false }, rec_two: null },
      rec_two: {
        rec_one: { rec_one: { rec_one: 1, rec_two: 2 }, rec_two: null },
        rec_two: { rec_one: 1, rec_two: null }
      }
    },

    Anything else I can help with?
  "#,
    baml_tyannotated!(Foo),
    baml_db!{
        class Foo {
            rec_one: (Foo | int | bool),
            rec_two: (Foo | int | bool | null),
        }
    },
    {
        "rec_one": {
            "rec_one": {
                "rec_one": true,
                "rec_two": false
            },
            "rec_two": null
        },
        "rec_two": {
            "rec_one": {
                "rec_one": {
                    "rec_one": 1,
                    "rec_two": 2
                },
                "rec_two": null
            },
            "rec_two": {
                "rec_one": 1,
                "rec_two": null
            }
        },
    }
);

// --- String in object with unescaped quotes ---

test_deserializer!(
    test_string_in_object_with_unescaped_quotes,
    r#"
    The answer is
    { rec_one: "and then i said \"hi\", and also \"bye\"", rec_two: "and then i said "hi", and also "bye"", "also_rec_one": ok },

    Anything else I can help with?
  "#,
    baml_tyannotated!(Foo),
    baml_db!{
        class Foo {
            rec_one: string,
            rec_two: string,
            also_rec_one: string,
        }
    },
    {
        "rec_one": "and then i said \"hi\", and also \"bye\"",
        "rec_two": "and then i said \"hi\", and also \"bye\"",
        "also_rec_one": "ok"
    }
);

// --- Array in object ---

test_deserializer!(
    test_array_in_object,
    r#"
    The answer is
    { rec_one: ["first with "quotes", and also "more"", "second"], rec_two: ["third", "fourth"] },
  "#,
    baml_tyannotated!(Foo),
    baml_db!{
        class Foo {
            rec_one: [string],
            rec_two: [string],
        }
    },
    {
        "rec_one": vec!["first with \"quotes\", and also \"more\"", "second"],
        "rec_two": vec!["third", "fourth"]
    }
);

// --- Enum without leading newline ---
// class WithFoo { foo Foo, name string }
// enum Foo { FOO, BAR }

test_deserializer!(
    test_enum_without_leading_newline,
    r#"
    {foo:FOO, name: "Greg"}
  "#,
    baml_tyannotated!(WithFoo),
    baml_db!{
        enum Foo { FOO, BAR }
        class WithFoo {
            foo: Foo,
            name: string,
        }
    },
    {
        "foo": "FOO",
        "name": "Greg"
    }
);

// --- Aliases with capitalized key ---

test_deserializer!(
    test_aliases_with_capitalized_key,
    // Key21 is now capitalized, but we should still be able to parse it.
    r#"{
      "key-dash": "This is a value with a dash",
      "Key21": "This is a value for key21",
      "key with space": "This is a value with space",
      "key4": "This is a value for key4",
      "key.with.punctuation/123": "This is a value with punctuation and numbers"
    }"#,
    baml_tyannotated!(TestClassAlias),
    baml_db!{
        class TestClassAlias {
            key: string @alias("key-dash"),
            key2: string @alias("key21"),
            key3: string @alias("key with space"),
            key4: string,
            key5: string @alias("key.with.punctuation/123"),
        }
    },
    {
        "key": "This is a value with a dash",
        "key2": "This is a value for key21",
        "key3": "This is a value with space",
        "key4": "This is a value for key4",
        "key5": "This is a value with punctuation and numbers"
    }
);

// --- Class with capitalization ---

test_deserializer!(
    test_class_with_capitalization,
    r#"{"Answer": {" content ": 78.54}}"#,
    baml_tyannotated!(SimpleTest),
    baml_db!{
        class Answer {
            content: float,
        }
        class SimpleTest {
            answer: Answer,
        }
    },
    {
        "answer": {
            "content": 78.54
        }
    }
);

// --- Skip field ---
// class SkipField { dont_skip string, skip_this_one string? @skip }
// @skip doesn't map to sap_model, so we model it as optional_field

test_deserializer!(
    test_skip_field,
    r#"{"dont_skip": "ok"}"#,
    baml_tyannotated!(SkipField),
    baml_db!{
        class SkipField {
            dont_skip: string,
            skip_this_one: (string | null) @class_completed_field_missing(null),
        }
    },
    {
        "dont_skip": "ok",
        "skip_this_one": null
    }
);

// Skipped tests:
// - test_optional_list (uses test_partial_deserializer_streaming! which doesn't exist)
// - test_integ_test_failure (uses test_partial_deserializer_streaming!)
// - test_string_in_object_with_unescaped_quotes_2 (uses test_partial_deserializer_streaming!)
// - test_string_in_object_with_unescaped_quotes_3 (uses test_partial_deserializer_streaming!)
// - test_partial_array_in_object (uses test_partial_deserializer_streaming!)
// - test_array_with_unescaped_quotes (uses test_partial_deserializer_streaming!)
