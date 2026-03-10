use super::*;
use crate::{baml_db, baml_tyannotated};

// --- Null tests ---

test_deserializer!(
    test_null,
    "null",
    baml_tyannotated!(null),
    baml_db! {},
    null
);

test_deserializer!(
    test_null_1,
    "null",
    baml_tyannotated!((string | null)),
    baml_db! {},
    null
);

test_deserializer!(
    test_null_2,
    "Null",
    baml_tyannotated!((string | null)),
    baml_db! {},
    // This is a string, not null
    "Null"
);

test_deserializer!(
    test_null_3,
    "None",
    baml_tyannotated!((string | null)),
    baml_db! {},
    // This is a string, not null
    "None"
);

// --- Number tests ---

test_deserializer!(
    test_number,
    "12111",
    baml_tyannotated!(int),
    baml_db! {},
    12111
);
test_deserializer!(
    test_number_2,
    "12,111",
    baml_tyannotated!(int),
    baml_db! {},
    12111
);

// --- String tests ---

test_deserializer!(
    test_string,
    r#""hello""#,
    baml_tyannotated!(string),
    baml_db! {},
    "\"hello\""
);

// --- Bool tests ---

test_deserializer!(
    test_bool,
    "true",
    baml_tyannotated!(bool),
    baml_db! {},
    true
);
test_deserializer!(
    test_bool_2,
    "True",
    baml_tyannotated!(bool),
    baml_db! {},
    true
);
test_deserializer!(
    test_bool_3,
    "false",
    baml_tyannotated!(bool),
    baml_db! {},
    false
);
test_deserializer!(
    test_bool_4,
    "False",
    baml_tyannotated!(bool),
    baml_db! {},
    false
);

test_deserializer!(
    test_bool_wrapped,
    "The answer is true",
    baml_tyannotated!([bool]),
    baml_db! {},
    [true]
);

test_deserializer!(
    test_bool_wrapped_mismatched_case,
    "The answer is True",
    baml_tyannotated!([bool]),
    baml_db! {},
    [true]
);

test_deserializer!(
    test_bool_wrapped_mismatched_case_preceded_by_text,
    "The tax return you provided has section for dependents.\n\nAnswer: **True**",
    baml_tyannotated!(bool),
    baml_db! {},
    true
);

test_deserializer!(
    test_bool_mismatched_case_followed_by_text,
    r#"False.\n\nThe statement "2 + 2 = 5" is mathematically incorrect. The correct sum of 2 + 2 is 4, not 5."#,
    baml_tyannotated!(bool),
    baml_db! {},
    false
);

test_failing_deserializer!(
    test_ambiguous_bool,
    "The answer is true or false",
    baml_tyannotated!(bool),
    baml_db! {}
);

test_failing_deserializer!(
    test_elaborate_ambiguous_bool,
    r#"False. The statement "2 + 2 = 5" is not accurate according to basic arithmetic. In standard arithmetic, the sum of 2 and 2 is equal to 4, not 5. Therefore, the statement does not hold true."#,
    baml_tyannotated!(bool),
    baml_db! {}
);

// --- Float tests ---

test_deserializer!(
    test_float,
    "12111.123",
    baml_tyannotated!(float),
    baml_db! {},
    12111.123
);

test_deserializer!(
    test_float_comma_us,
    "12,111.123",
    baml_tyannotated!(float),
    baml_db! {},
    12111.123
);

test_deserializer!(
    test_float_comma_german2,
    "12.11.",
    baml_tyannotated!(float),
    baml_db! {},
    12.11
);

test_deserializer!(
    test_float_1,
    "1/5",
    baml_tyannotated!(float),
    baml_db! {},
    0.2
);

// --- Array tests ---

test_deserializer!(
    test_array,
    r#"[1, 2, 3]"#,
    baml_tyannotated!([int]),
    baml_db! {},
    [1, 2, 3]
);

test_deserializer!(
    test_array_1,
    r#"[1, 2, 3]"#,
    baml_tyannotated!([string]),
    baml_db! {},
    ["1", "2", "3"]
);

test_deserializer!(
    test_array_3,
    r#"[1, 2, 3]"#,
    baml_tyannotated!([float]),
    baml_db! {},
    [1., 2., 3.]
);

test_deserializer!(
    test_string_to_float_from_comma_separated,
    "1 cup unsalted butter, room temperature",
    baml_tyannotated!(float),
    baml_db! {},
    1.0
);

// --- Object / class tests ---

test_deserializer!(
    test_object,
    r#"{"key": "value"}"#,
    baml_tyannotated!(Test),
    baml_db!{
        class Test {
            key: string,
        }
    },
    {"key": "value"}
);

test_deserializer!(
    test_nested,
    r#"{"key": [1, 2, 3]}"#,
    baml_tyannotated!(Test),
    baml_db!{
        class Test {
            key: [int],
        }
    },
    {"key": [1, 2, 3]}
);

test_deserializer!(
    test_nested_whitespace,
    r#" { "key" : [ 1 , 2 , 3 ] } "#,
    baml_tyannotated!(Test),
    baml_db!{
        class Test {
            key: [int],
        }
    },
    {"key": [1, 2, 3]}
);

test_deserializer!(
    test_nested_whitespace_prefix_suffix,
    r#"prefix { "key" : [ 1 , 2 , 3 ] } suffix"#,
    baml_tyannotated!(Test),
    baml_db!{
        class Test {
            key: [int],
        }
    },
    {"key": [1, 2, 3]}
);

// --- Multiple top level objects ---

test_deserializer!(
    test_multiple_top_level_1,
    r#"{"key": "value1"} {"key": "value2"}"#,
    baml_tyannotated!(Test),
    baml_db!{
        class Test {
            key: string,
        }
    },
    {"key": "value1"}
);

test_deserializer!(
    test_multiple_top_level_2,
    r#"{"key": "value1"} {"key": "value2"}"#,
    baml_tyannotated!([Test]),
    baml_db!{
        class Test {
            key: string,
        }
    },
    [{"key": "value1"}, {"key": "value2"}]
);

test_deserializer!(
    test_multiple_top_level_prefix_suffix_1,
    r#"prefix {"key": "value1"} some random text {"key": "value2"} suffix"#,
    baml_tyannotated!(Test),
    baml_db!{
        class Test {
            key: string,
        }
    },
    {"key": "value1"}
);

test_deserializer!(
    test_multiple_top_level_prefix_suffix_2,
    r#"prefix {"key": "value1"} some random text {"key": "value2"} suffix"#,
    baml_tyannotated!([Test]),
    baml_db!{
        class Test {
            key: string,
        }
    },
    [{"key": "value1"}, {"key": "value2"}]
);

// --- Trailing comma ---

test_deserializer!(
    test_trailing_comma_array_2,
    r#"[1, 2, 3,]"#,
    baml_tyannotated!([int]),
    baml_db! {},
    [1, 2, 3]
);

test_deserializer!(
    test_trailing_comma_array_3,
    r#"[1, 2, 3,]"#,
    baml_tyannotated!([string]),
    baml_db! {},
    ["1", "2", "3"]
);

test_deserializer!(
    test_trailing_comma_object,
    r#"{"key": "value",}"#,
    baml_tyannotated!(Test),
    baml_db!{
        class Test {
            key: string,
        }
    },
    {"key": "value"}
);

// --- Invalid JSONish ---

test_deserializer!(
    test_invalid_array,
    r#"[1, 2, 3"#,
    baml_tyannotated!([int]),
    baml_db! {},
    [1, 2, 3]
);

test_deserializer!(
    test_invalid_array_in_object,
    r#"{"key": [1, 2, 3"#,
    baml_tyannotated!(Test),
    baml_db!{
        class Test {
            key: [int],
        }
    },
    {"key": [1, 2, 3]}
);

test_deserializer!(
    test_incomplete_string,
    r#""hello"#,
    baml_tyannotated!(string),
    baml_db! {},
    "\"hello"
);

test_deserializer!(
    test_incomplete_string_in_object,
    r#"{"key": "value"#,
    baml_tyannotated!(Test),
    baml_db!{
        class Test {
            key: string,
        }
    },
    {"key": "value"}
);

test_deserializer!(
    test_prefixed_incompleted_string,
    r#"prefix "hello"#,
    baml_tyannotated!(string),
    baml_db! {},
    "prefix \"hello"
);

// --- Large object with nested class ---

test_deserializer!(
    test_large_object,
    r#"{"key": "value", "array": [1, 2, 3], "object": {"key": "value"}}"#,
    baml_tyannotated!(Test),
    baml_db!{
        class Foo {
            key: string,
        }
        class Test {
            key: string,
            array: [int],
            object: Foo,
        }
    },
    {"key": "value", "array": [1, 2, 3], "object": {"key": "value"}}
);

// --- Markdown JSON examples ---

test_deserializer!(
    test_json_md_example_1,
    r#"
  some text
  ```json
  {
    "key": "value",
    "array": [1, 2, 3],
    "object": {
      "key": "value"
    }
  }
  ```
  "#,
    baml_tyannotated!(Test),
    baml_db!{
        class Foo {
            key: string,
        }
        class Test {
            key: string,
            array: [int],
            object: Foo,
        }
    },
    {"key": "value", "array": [1, 2, 3], "object": {"key": "value"}}
);

test_deserializer!(
    test_json_md_example_2,
    r#"
  some text
  ```json
  {
    "key": "value",
    "array": [1, 2, 3],
    "object": {
      "key": "value"
    }
  }
  ```


  ```json
  ["1", "2"]
  ```
  "#,
    baml_tyannotated!(Test),
    baml_db!{
        class Foo {
            key: string,
        }
        class Test {
            key: string,
            array: [int],
            object: Foo,
        }
    },
    {"key": "value", "array": [1, 2, 3], "object": {"key": "value"}}
);

test_deserializer!(
    test_json_md_example_3,
    // This test uses int list as target, not the class
    r#"
  some text
  ```json
  {
    "key": "value",
    "array": [1, 2, 3],
    "object": {
      "key": "value"
    }
  }
  ```


  ```json
  ["1", "2"]
  ```
  "#,
    baml_tyannotated!([int]),
    baml_db! {},
    [1, 2]
);

test_deserializer!(
    test_json_md_example_1_bad_inner_json,
    r#"
  some text
  ```json
  {
    "key": "value",
    "array": [1, 2, 3,],
    "object": {
      "key": "value"
    }
  }
  ```
  "#,
    baml_tyannotated!(Test),
    baml_db!{
        class Foo {
            key: string,
        }
        class Test {
            key: string,
            array: [int],
            object: Foo,
        }
    },
    {"key": "value", "array": [1, 2, 3], "object": {"key": "value"}}
);

test_deserializer!(
    test_json_md_example_1_bad_inner_json_2,
    r#"
  some text
  ```json
  {
    "key": "value",
    "array": [1, 2, 3, "somet"string with quotes"],
    "object": {
      "key": "value"
    }
  }
  ```
  "#,
    baml_tyannotated!(Test),
    baml_db!{
        class Foo {
            key: string,
        }
        class Test {
            key: string,
            array: [(int | string)],
            object: Foo,
        }
    },
    {"key": "value", "array": [1, 2, 3, "somet\"string with quotes"], "object": {"key": "value"}}
);

test_deserializer!(
    test_json_md_example_1_bad_inner_json_3,
    r#"
  some text
  ```json
  {
    "key": "value",
    "array": [1, 2, 3, 'some stinrg'   with quotes' /* test */],
    "object": { // Test comment
      "key": "value"
    },
  }
  ```
  "#,
    baml_tyannotated!(Test),
    baml_db!{
        class Foo {
            key: string,
        }
        class Test {
            key: string,
            array: [(int | string)],
            object: Foo,
        }
    },
    {"key": "value", "array": [1, 2, 3, "some stinrg'   with quotes"], "object": {"key": "value"}}
);

test_deserializer!(
    test_unquoted_keys,
    r#"
  some text
  ```json
  {
    key: "value",
    array: [1, 2, 3, 'some stinrg'   with quotes' /* test */],
    object: { // Test comment
      key: "value"
    },
  }
  ```
  "#,
    baml_tyannotated!(Test),
    baml_db!{
        class Foo {
            key: string,
        }
        class Test {
            key: string,
            array: [(int | string)],
            object: Foo,
        }
    },
    {"key": "value", "array": [1, 2, 3, "some stinrg'   with quotes"], "object": {"key": "value"}}
);

test_deserializer!(
    test_json_with_unquoted_values_with_spaces,
    r#"
  {
    key: value with space,
    array: [1, 2, 3],
    object: {
      key: value
    }
  }
  "#,
    baml_tyannotated!(Test),
    baml_db!{
        class Foo {
            key: string,
        }
        class Test {
            key: string,
            array: [(int | string)],
            object: Foo,
        }
    },
    {"key": "value with space", "array": [1, 2, 3], "object": {"key": "value"}}
);

test_deserializer!(
    test_json_with_unquoted_values_with_spaces_and_new_lines,
    r#"
  {
    key: "test a long
thing with new

lines",
    array: [1, 2, 3],
    object: {
      key: value
    }
  }
  "#,
    baml_tyannotated!(Test),
    baml_db!{
        class Foo {
            key: string,
        }
        class Test {
            key: string,
            array: [(int | string)],
            object: Foo,
        }
    },
    {"key": "test a long\nthing with new\n\nlines", "array": [1, 2, 3], "object": {"key": "value"}}
);

test_deserializer!(
    test_json_with_markdown_without_quotes,
    r#"
  {
    "my_field_0": true,
    "my_field_1": **First fragment, Another fragment**

Frag 2, frag 3. Frag 4, Frag 5, Frag 5.

Frag 6, the rest, of the sentence. Then i would quote something "like this" or this.

Then would add a summary of sorts.
  }
  "#,
    baml_tyannotated!(Test),
    baml_db!{
        class Test {
            my_field_0: bool,
            my_field_1: string,
        }
    },
    {
      "my_field_0": true,
      "my_field_1": "**First fragment, Another fragment**\n\nFrag 2, frag 3. Frag 4, Frag 5, Frag 5.\n\nFrag 6, the rest, of the sentence. Then i would quote something \"like this\" or this.\n\nThen would add a summary of sorts."
    }
);

// --- Whitespace in keys ---

test_deserializer!(
    test_whitespace_in_keys_preserved,
    r#"{" answer ": {" content ": 78.54}}"#,
    baml_tyannotated!(string),
    baml_db! {},
    r#"{" answer ": {" content ": 78.54}}"#
);

test_deserializer!(
    test_class_with_whitespace_keys,
    r#"{" answer ": {" content ": 78.54}}"#,
    baml_tyannotated!(Test),
    baml_db!{
        class Answer {
            content: float,
        }
        class Test {
            answer: Answer,
        }
    },
    {"answer": {"content": 78.54}}
);

// --- Malformed JSON sequence (partial test) ---

#[test]
fn test_mal_formed_json_sequence() {
    let db = crate::baml_db! {
        class Foo1 {
            field1: (string | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
            field2: (string | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
            field3: (string | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
            field4: (string | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
            field5: (string | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
            field6: (string | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
        }
        class Foo2 {
            field7: (string | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
            field8: (string | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
            field9: (string | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
            field10: (string | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
            field11: (string | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
            field12: (string | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
            field13: (string | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
            field14: (string | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
            field15: (string | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
            field16: (string | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
            field17: (string | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
            field18: (string | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
            field19: (string | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
            field20: (string | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
            field21: (string | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
            field22: (string | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
            field23: (string | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
            field24: (string | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
            field25: (string | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
        }
        class Foo3 {
            field28: (string | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
            field29: [string] @class_in_progress_field_missing([]) @class_completed_field_missing([]),
            field30: [string] @class_in_progress_field_missing([]) @class_completed_field_missing([]),
            field31: [string] @class_in_progress_field_missing([]) @class_completed_field_missing([]),
            field32: [string] @class_in_progress_field_missing([]) @class_completed_field_missing([]),
            field33: (string | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
            field34: (string | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
            field35: (string | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
            field36: (string | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
        }
        class Test {
            foo1: (Foo1 | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
            foo2: [Foo2] @class_in_progress_field_missing([]) @class_completed_field_missing([]),
            foo3: (Foo3 | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
        }
    };

    let raw = r#"```json
{
"foo1": {
"field1": "Something horrible has happened!!",
"field2": null,
"field3": null,
"field4": null,
"field5": null,
"field6": null
},
"foo2": {
"field7": null,
"field8": null,
"field9": null,
"field10": null,
"field11": null,
"field12": null,
"field13": null{
"foo1": {
"field1": "A thing has been going on poorly",
"field2": null,
"field3": null,
"field4": null,
"field5": null,
"field6": null
},
"foo2": {
"field7": null,
"field8": null,
"field9": null,
"field10": null,
"field11": null,
"field12": null,
"field13": null,
"field14": null,
"field15": null,
"field16": null,
"field17": null,
"field18": null,
"field19": null,
"field20": null,
"field21": null,
"field22": null,
"field23": null,
"field24": null,
"field25": null
},
"foo2": [
{
  "field26": "The bad thing is confirmed.",
  "field27": null
}
],
"foo3": {
"field28": "We are really going to try and take care of the bad thing.",
"field29": [],
"field30": [],
"field31": [],
"field32": [],
"field33": null,
"field34": null,
"field35": null,
"field36": null
}
}"#;

    // Use is_done=false for partial streaming parse
    let target_ty = crate::baml_tyannotated!(Test);
    let target_ty = db.resolve_with_meta(target_ty.as_ref()).unwrap();
    let parsed =
        crate::jsonish::parse(raw, crate::jsonish::ParseOptions::default(), false).unwrap();
    let ctx = crate::deserializer::coercer::ParsingContext::new(&db);
    let result = TyResolvedRef::coerce(&ctx, target_ty, &parsed);
    assert!(result.is_ok(), "Failed to parse: {result:?}");
    let value = result.unwrap().unwrap();
    let json_value = serde_json::to_value(&value).unwrap();
    let expected = serde_json::json!({
      "foo1": {
        "field1": "Something horrible has happened!!",
        "field2": null,
        "field3": null,
        "field4": null,
        "field5": null,
        "field6": null
      },
      "foo2": [
        {
          "field7": null,
          "field8": null,
          "field9": null,
          "field10": null,
          "field11": null,
          "field12": null,
          "field13": "null{\n\"foo1\": {\n\"field1\": \"A thing has been going on poorly\"",
          "field14": null,
          "field15": null,
          "field16": null,
          "field17": null,
          "field18": null,
          "field19": null,
          "field20": null,
          "field21": null,
          "field22": null,
          "field23": null,
          "field24": null,
          "field25": null
        }
      ],
      "foo3": {
        "field28": "We are really going to try and take care of the bad thing.",
        "field29": [],
        "field30": [],
        "field31": [],
        "field32": [],
        "field33": null,
        "field34": null,
        "field35": null,
        "field36": null
      }
    });
    assert_eq!(json_value, expected);
}

// --- Localization tests ---

test_deserializer!(
    test_localization,
    r#"
To effectively localize these strings for a Portuguese-speaking audience, I will focus on maintaining the original tone and meaning while ensuring that the translations sound natural and culturally appropriate. For the game title "Arcadian Atlas," I will keep it unchanged as it is a proper noun and likely a branded term within the game. For the other strings, I will adapt them to resonate with Portuguese players, using idiomatic expressions if necessary and ensuring that the sense of adventure and urgency is conveyed.

For the string with the placeholder {player_name}, I will ensure that the placeholder is kept intact and that the surrounding text is grammatically correct and flows naturally in Portuguese. The name "Jonathan" will remain unchanged as it is a proper noun and recognizable in Portuguese.

JSON Output:
```
[
  {
    "id": "CH1_Welcome",
    "English": "Welcome to Arcadian Atlas",
    "Portuguese": "Bem-vindo ao Arcadian Atlas"
  },
  {
    "id": "CH1_02",
    "English": "Arcadia is a vast land, with monsters and dangers!",
    "Portuguese": "Arcadia é uma terra vasta, repleta de monstros e perigos!"
  },
  {
    "id": "CH1_03",
    "English": "Find him {player_name}. Find him and save Arcadia. Jonathan will save us all. It is the only way.",
    "Portuguese": "Encontre-o {player_name}. Encontre-o e salve Arcadia. Jonathan nos salvará a todos. É a única maneira."
  }
]
```"#,
    baml_tyannotated!([Test]),
    baml_db!{
        class Test {
            id: string,
            English: string,
            Portuguese: string,
        }
    },
    [
        {
            "id": "CH1_Welcome",
            "English": "Welcome to Arcadian Atlas",
            "Portuguese": "Bem-vindo ao Arcadian Atlas"
        },
        {
            "id": "CH1_02",
            "English": "Arcadia is a vast land, with monsters and dangers!",
            "Portuguese": "Arcadia é uma terra vasta, repleta de monstros e perigos!"
        },
        {
            "id": "CH1_03",
            "English": "Find him {player_name}. Find him and save Arcadia. Jonathan will save us all. It is the only way.",
            "Portuguese": "Encontre-o {player_name}. Encontre-o e salve Arcadia. Jonathan nos salvará a todos. É a única maneira."
        }
    ]
);

test_deserializer!(
    test_localization2,
    r#"
To effectively localize these strings for a Portuguese-speaking audience, I will focus on maintaining the original tone and meaning while ensuring that the translations sound natural and culturally appropriate. For the game title "Arcadian Atlas," I will keep it unchanged as it is a proper noun and likely a branded term within the game. For the other strings, I will adapt them to resonate with Portuguese players, using idiomatic expressions if necessary and ensuring that the sense of adventure and urgency is conveyed.

For the string with the placeholder {player_name}, I will ensure that the placeholder is kept intact and that the surrounding text is grammatically correct and flows naturally in Portuguese. The name "Jonathan" will remain unchanged as it is a proper noun and recognizable in Portuguese.


[
  {
    id: "CH1_Welcome",
    English: "Welcome to Arcadian Atlas",
    Portuguese: "Bem-vindo ao Arcadian Atlas"
  },
  {
    id: "CH1_02",
    English: "Arcadia is a vast land, with monsters and dangers!",
    Portuguese: """Arcadia é uma terra vasta,

repleta de monstros e perigos!"""
  },
  {
    id: "CH1_03",
    English: "Find him {player_name}. Find him and save Arcadia. Jonathan will save us all. It is the only way.",
  }
]"#,
    baml_tyannotated!([Test]),
    baml_db!{
        class Test {
            id: string,
            English: string,
            Portuguese: (string | null) @class_completed_field_missing(null),
        }
    },
    [
        {
            "id": "CH1_Welcome",
            "English": "Welcome to Arcadian Atlas",
            "Portuguese": "Bem-vindo ao Arcadian Atlas"
        },
        {
            "id": "CH1_02",
            "English": "Arcadia is a vast land, with monsters and dangers!",
            "Portuguese": "Arcadia é uma terra vasta,\n\nrepleta de monstros e perigos!"
        },
        {
            "id": "CH1_03",
            "English": "Find him {player_name}. Find him and save Arcadia. Jonathan will save us all. It is the only way.",
            "Portuguese": null
        }
    ]
);

// --- SIDD test ---

test_deserializer!(
    test_sidd,
    r#"
<thinking>
To create a personalized catalogue for the customer, I need to analyze both the properties available and the customer's requirements. The customer is looking for an apartment that is 970.0 sq.ft. and costs Rs. 27,030,000.00. However, none of the listed properties match these specifications perfectly.

1. **Analyze the Properties**: I'll look at the properties provided to identify common themes, features, or unique selling points that can inspire creative headings.
2. **Consider Customer Requirements**: While the customer has specific requirements, the task is to create headings that are creative and interesting, not strictly based on those requirements.
3. **Generate Creative Headings**: I will brainstorm seven catchy headings that can be used to categorize the properties in a way that highlights their best features or unique aspects.

Next, I will generate the headings and their corresponding Python functions to categorize the properties.
</thinking>

<reflection>
I have considered the properties and the customer's requirements. The next step is to formulate creative headings that reflect the unique aspects of the properties without being overly focused on the customer's specific requirements. I will ensure that each heading is distinct and engaging.
</reflection>

<thinking>
Here are the seven creative headings along with their descriptions and Python functions:

1. **Urban Oasis**
   - This heading captures properties that offer a serene living experience amidst the bustling city life.
   - Python function:
   ```python
   def is_urban_oasis(property):
       return 'Large Green Area' in property['amenities'] or 'Garden' in property['amenities']
   ```

   Now, I will compile these into the required format.
</thinking>

{
  "headings": [
    {
      "heading": "Urban Oasis",
      "python_function_code": """def is_urban_oasis(property):
       return 'Large Green Area' in property['amenities'] or 'Garden' in property['amenities']""",
      "description": "Properties that offer a serene living experience amidst the bustling city life."
    }
  ]
}
  "#,
    baml_tyannotated!(Headings),
    baml_db!{
        class Heading {
            heading: string,
            python_function_code: string,
            description: string,
        }
        class Headings {
            headings: [Heading],
        }
    },
    {
      "headings": [
        {
          "heading": "Urban Oasis",
          "python_function_code": "def is_urban_oasis(property):\n       return 'Large Green Area' in property['amenities'] or 'Garden' in property['amenities']",
          "description": "Properties that offer a serene living experience amidst the bustling city life."
        }
      ]
    }
);

test_deserializer!(
    test_injected_triple_quoted_string,
    r#"
{
  "headings": [
    {
      "heading": "Urban Oasis",
      "python_function_code": """def is_urban_oasis(property):
       return 'Large Green Area' in property['amenities'] or 'Garden' in property['amenities']""",
      "description": "Properties that offer a serene living experience amidst the bustling city life."
    }
  ]
}
  "#,
    baml_tyannotated!(Headings),
    baml_db!{
        class Heading {
            heading: string,
            python_function_code: string,
            description: string,
        }
        class Headings {
            headings: [Heading],
        }
    },
    {
        "headings": [
            {
                "heading": "Urban Oasis",
                "python_function_code": "def is_urban_oasis(property):\n       return 'Large Green Area' in property['amenities'] or 'Garden' in property['amenities']",
                "description": "Properties that offer a serene living experience amidst the bustling city life."
            }
        ]
    }
);
