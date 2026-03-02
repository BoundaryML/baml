use super::*;

// --- Null tests ---

test_deserializer!(test_null, "null", null_ty(), empty_db(), null);

test_deserializer!(test_null_1, "null", optional(string_ty()), empty_db(), null);

test_deserializer!(
    test_null_2,
    "Null",
    optional(string_ty()),
    empty_db(),
    // This is a string, not null
    "Null"
);

test_deserializer!(
    test_null_3,
    "None",
    optional(string_ty()),
    empty_db(),
    // This is a string, not null
    "None"
);

// --- Number tests ---

test_deserializer!(test_number, "12111", int_ty(), empty_db(), 12111);
test_deserializer!(test_number_2, "12,111", int_ty(), empty_db(), 12111);

// --- String tests ---

test_deserializer!(
    test_string,
    r#""hello""#,
    string_ty(),
    empty_db(),
    "\"hello\""
);

// --- Bool tests ---

test_deserializer!(test_bool, "true", bool_ty(), empty_db(), true);
test_deserializer!(test_bool_2, "True", bool_ty(), empty_db(), true);
test_deserializer!(test_bool_3, "false", bool_ty(), empty_db(), false);
test_deserializer!(test_bool_4, "False", bool_ty(), empty_db(), false);

test_deserializer!(
    test_bool_wrapped,
    "The answer is true",
    array_of(annotated(bool_ty())),
    empty_db(),
    [true]
);

test_deserializer!(
    test_bool_wrapped_mismatched_case,
    "The answer is True",
    array_of(annotated(bool_ty())),
    empty_db(),
    [true]
);

test_deserializer!(
    test_bool_wrapped_mismatched_case_preceded_by_text,
    "The tax return you provided has section for dependents.\n\nAnswer: **True**",
    bool_ty(),
    empty_db(),
    true
);

test_deserializer!(
    test_bool_mismatched_case_followed_by_text,
    r#"False.\n\nThe statement "2 + 2 = 5" is mathematically incorrect. The correct sum of 2 + 2 is 4, not 5."#,
    bool_ty(),
    empty_db(),
    false
);

test_failing_deserializer!(
    test_ambiguous_bool,
    "The answer is true or false",
    bool_ty(),
    empty_db()
);

test_failing_deserializer!(
    test_elaborate_ambiguous_bool,
    r#"False. The statement "2 + 2 = 5" is not accurate according to basic arithmetic. In standard arithmetic, the sum of 2 and 2 is equal to 4, not 5. Therefore, the statement does not hold true."#,
    bool_ty(),
    empty_db()
);

// --- Float tests ---

test_deserializer!(test_float, "12111.123", float_ty(), empty_db(), 12111.123);

test_deserializer!(
    test_float_comma_us,
    "12,111.123",
    float_ty(),
    empty_db(),
    12111.123
);

test_deserializer!(
    test_float_comma_german2,
    "12.11.",
    float_ty(),
    empty_db(),
    12.11
);

test_deserializer!(test_float_1, "1/5", float_ty(), empty_db(), 0.2);

// --- Array tests ---

test_deserializer!(
    test_array,
    r#"[1, 2, 3]"#,
    array_of(annotated(int_ty())),
    empty_db(),
    [1, 2, 3]
);

test_deserializer!(
    test_array_1,
    r#"[1, 2, 3]"#,
    array_of(annotated(string_ty())),
    empty_db(),
    ["1", "2", "3"]
);

test_deserializer!(
    test_array_3,
    r#"[1, 2, 3]"#,
    array_of(annotated(float_ty())),
    empty_db(),
    [1., 2., 3.]
);

test_deserializer!(
    test_string_to_float_from_comma_separated,
    "1 cup unsalted butter, room temperature",
    float_ty(),
    empty_db(),
    1.0
);

// --- Object / class tests ---

test_deserializer!(
    test_object,
    r#"{"key": "value"}"#,
    class_ty("Test", vec![field("key", string_ty())]),
    empty_db(),
    {"key": "value"}
);

test_deserializer!(
    test_nested,
    r#"{"key": [1, 2, 3]}"#,
    class_ty("Test", vec![field("key", array_of(annotated(int_ty())))]),
    empty_db(),
    {"key": [1, 2, 3]}
);

test_deserializer!(
    test_nested_whitespace,
    r#" { "key" : [ 1 , 2 , 3 ] } "#,
    class_ty("Test", vec![field("key", array_of(annotated(int_ty())))]),
    empty_db(),
    {"key": [1, 2, 3]}
);

test_deserializer!(
    test_nested_whitespace_prefix_suffix,
    r#"prefix { "key" : [ 1 , 2 , 3 ] } suffix"#,
    class_ty("Test", vec![field("key", array_of(annotated(int_ty())))]),
    empty_db(),
    {"key": [1, 2, 3]}
);

// --- Multiple top level objects ---

test_deserializer!(
    test_multiple_top_level_1,
    r#"{"key": "value1"} {"key": "value2"}"#,
    class_ty("Test", vec![field("key", string_ty())]),
    empty_db(),
    {"key": "value1"}
);

test_deserializer!(
    test_multiple_top_level_2,
    r#"{"key": "value1"} {"key": "value2"}"#,
    array_of(annotated(class_ty("Test", vec![field("key", string_ty())]))),
    empty_db(),
    [{"key": "value1"}, {"key": "value2"}]
);

test_deserializer!(
    test_multiple_top_level_prefix_suffix_1,
    r#"prefix {"key": "value1"} some random text {"key": "value2"} suffix"#,
    class_ty("Test", vec![field("key", string_ty())]),
    empty_db(),
    {"key": "value1"}
);

test_deserializer!(
    test_multiple_top_level_prefix_suffix_2,
    r#"prefix {"key": "value1"} some random text {"key": "value2"} suffix"#,
    array_of(annotated(class_ty("Test", vec![field("key", string_ty())]))),
    empty_db(),
    [{"key": "value1"}, {"key": "value2"}]
);

// --- Trailing comma ---

test_deserializer!(
    test_trailing_comma_array_2,
    r#"[1, 2, 3,]"#,
    array_of(annotated(int_ty())),
    empty_db(),
    [1, 2, 3]
);

test_deserializer!(
    test_trailing_comma_array_3,
    r#"[1, 2, 3,]"#,
    array_of(annotated(string_ty())),
    empty_db(),
    ["1", "2", "3"]
);

test_deserializer!(
    test_trailing_comma_object,
    r#"{"key": "value",}"#,
    class_ty("Test", vec![field("key", string_ty())]),
    empty_db(),
    {"key": "value"}
);

// --- Invalid JSONish ---

test_deserializer!(
    test_invalid_array,
    r#"[1, 2, 3"#,
    array_of(annotated(int_ty())),
    empty_db(),
    [1, 2, 3]
);

test_deserializer!(
    test_invalid_array_in_object,
    r#"{"key": [1, 2, 3"#,
    class_ty("Test", vec![field("key", array_of(annotated(int_ty())))]),
    empty_db(),
    {"key": [1, 2, 3]}
);

test_deserializer!(
    test_incomplete_string,
    r#""hello"#,
    string_ty(),
    empty_db(),
    "\"hello"
);

test_deserializer!(
    test_incomplete_string_in_object,
    r#"{"key": "value"#,
    class_ty("Test", vec![field("key", string_ty())]),
    empty_db(),
    {"key": "value"}
);

test_deserializer!(
    test_prefixed_incompleted_string,
    r#"prefix "hello"#,
    string_ty(),
    empty_db(),
    "prefix \"hello"
);

// --- Large object with nested class ---

#[test]
fn test_large_object() {
    let db = crate::baml_db! {
        class Foo { key: string }
        class Test { key: string, array: [int], object: Foo }
    };

    let raw = r#"{"key": "value", "array": [1, 2, 3], "object": {"key": "value"}}"#;
    let target_ty = db.resolved_from_ident(&"Test").unwrap();
    let parsed = crate::jsonish::parse(raw, Default::default(), true).unwrap();
    let ctx = crate::deserializer::coercer::ParsingContext::new(target_ty, &db);
    let annots = TypeAnnotations::default();
    let target = TyWithMeta::new(target_ty, &annots);
    let result = TyResolvedRef::coerce(&ctx, target, &parsed);
    assert!(result.is_ok(), "Failed to parse: {:?}", result);
    let value = result.unwrap().unwrap();
    let json_value = serde_json::to_value(&value).unwrap();
    let expected =
        serde_json::json!({"key": "value", "array": [1, 2, 3], "object": {"key": "value"}});
    assert_eq!(json_value, expected);
}

// --- Markdown JSON examples ---

#[test]
fn test_json_md_example_1() {
    let db = crate::baml_db! {
        class Foo { key: string }
        class Test { key: string, array: [int], object: Foo }
    };

    let raw = r#"
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
  "#;
    let target_ty = db.resolved_from_ident(&"Test").unwrap();
    let parsed = crate::jsonish::parse(raw, Default::default(), true).unwrap();
    let ctx = crate::deserializer::coercer::ParsingContext::new(target_ty, &db);
    let annots = TypeAnnotations::default();
    let target = TyWithMeta::new(target_ty, &annots);
    let result = TyResolvedRef::coerce(&ctx, target, &parsed);
    assert!(result.is_ok(), "Failed to parse: {:?}", result);
    let value = result.unwrap().unwrap();
    let json_value = serde_json::to_value(&value).unwrap();
    let expected =
        serde_json::json!({"key": "value", "array": [1, 2, 3], "object": {"key": "value"}});
    assert_eq!(json_value, expected);
}

#[test]
fn test_json_md_example_2() {
    let db = crate::baml_db! {
        class Foo { key: string }
        class Test { key: string, array: [int], object: Foo }
    };

    let raw = r#"
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
  "#;
    let target_ty = db.resolved_from_ident(&"Test").unwrap();
    let parsed = crate::jsonish::parse(raw, Default::default(), true).unwrap();
    let ctx = crate::deserializer::coercer::ParsingContext::new(target_ty, &db);
    let annots = TypeAnnotations::default();
    let target = TyWithMeta::new(target_ty, &annots);
    let result = TyResolvedRef::coerce(&ctx, target, &parsed);
    assert!(result.is_ok(), "Failed to parse: {:?}", result);
    let value = result.unwrap().unwrap();
    let json_value = serde_json::to_value(&value).unwrap();
    let expected =
        serde_json::json!({"key": "value", "array": [1, 2, 3], "object": {"key": "value"}});
    assert_eq!(json_value, expected);
}

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
    crate::baml_tyresolved!([int]),
    empty_db(),
    [1, 2]
);

#[test]
fn test_json_md_example_1_bad_inner_json() {
    let db = crate::baml_db! {
        class Foo { key: string }
        class Test { key: string, array: [int], object: Foo }
    };

    let raw = r#"
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
  "#;
    let target_ty = db.resolved_from_ident(&"Test").unwrap();
    let parsed = crate::jsonish::parse(raw, Default::default(), true).unwrap();
    let ctx = crate::deserializer::coercer::ParsingContext::new(target_ty, &db);
    let annots = TypeAnnotations::default();
    let target = TyWithMeta::new(target_ty, &annots);
    let result = TyResolvedRef::coerce(&ctx, target, &parsed);
    assert!(result.is_ok(), "Failed to parse: {:?}", result);
    let value = result.unwrap().unwrap();
    let json_value = serde_json::to_value(&value).unwrap();
    let expected =
        serde_json::json!({"key": "value", "array": [1, 2, 3], "object": {"key": "value"}});
    assert_eq!(json_value, expected);
}

#[test]
fn test_json_md_example_1_bad_inner_json_2() {
    let db = crate::baml_db! {
        class Foo { key: string }
        class Test { key: string, array: [(int | string)], object: Foo }
    };

    let raw = r#"
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
  "#;
    let target_ty = db.resolved_from_ident(&"Test").unwrap();
    let parsed = crate::jsonish::parse(raw, Default::default(), true).unwrap();
    let ctx = crate::deserializer::coercer::ParsingContext::new(target_ty, &db);
    let annots = TypeAnnotations::default();
    let target = TyWithMeta::new(target_ty, &annots);
    let result = TyResolvedRef::coerce(&ctx, target, &parsed);
    assert!(result.is_ok(), "Failed to parse: {:?}", result);
    let value = result.unwrap().unwrap();
    let json_value = serde_json::to_value(&value).unwrap();
    let expected = serde_json::json!({"key": "value", "array": [1, 2, 3, "somet\"string with quotes"], "object": {"key": "value"}});
    assert_eq!(json_value, expected);
}

#[test]
fn test_json_md_example_1_bad_inner_json_3() {
    let db = crate::baml_db! {
        class Foo { key: string }
        class Test { key: string, array: [(int | string)], object: Foo }
    };

    let raw = r#"
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
  "#;
    let target_ty = db.resolved_from_ident(&"Test").unwrap();
    let parsed = crate::jsonish::parse(raw, Default::default(), true).unwrap();
    let ctx = crate::deserializer::coercer::ParsingContext::new(target_ty, &db);
    let annots = TypeAnnotations::default();
    let target = TyWithMeta::new(target_ty, &annots);
    let result = TyResolvedRef::coerce(&ctx, target, &parsed);
    assert!(result.is_ok(), "Failed to parse: {:?}", result);
    let value = result.unwrap().unwrap();
    let json_value = serde_json::to_value(&value).unwrap();
    let expected = serde_json::json!({"key": "value", "array": [1, 2, 3, "some stinrg'   with quotes"], "object": {"key": "value"}});
    assert_eq!(json_value, expected);
}

#[test]
fn test_unquoted_keys() {
    let db = crate::baml_db! {
        class Foo { key: string }
        class Test { key: string, array: [(int | string)], object: Foo }
    };

    let raw = r#"
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
  "#;
    let target_ty = db.resolved_from_ident(&"Test").unwrap();
    let parsed = crate::jsonish::parse(raw, Default::default(), true).unwrap();
    let ctx = crate::deserializer::coercer::ParsingContext::new(target_ty, &db);
    let annots = TypeAnnotations::default();
    let target = TyWithMeta::new(target_ty, &annots);
    let result = TyResolvedRef::coerce(&ctx, target, &parsed);
    assert!(result.is_ok(), "Failed to parse: {:?}", result);
    let value = result.unwrap().unwrap();
    let json_value = serde_json::to_value(&value).unwrap();
    let expected = serde_json::json!({"key": "value", "array": [1, 2, 3, "some stinrg'   with quotes"], "object": {"key": "value"}});
    assert_eq!(json_value, expected);
}

#[test]
fn test_json_with_unquoted_values_with_spaces() {
    let db = crate::baml_db! {
        class Foo { key: string }
        class Test { key: string, array: [(int | string)], object: Foo }
    };

    let raw = r#"
  {
    key: value with space,
    array: [1, 2, 3],
    object: {
      key: value
    }
  }
  "#;
    let target_ty = db.resolved_from_ident(&"Test").unwrap();
    let parsed = crate::jsonish::parse(raw, Default::default(), true).unwrap();
    let ctx = crate::deserializer::coercer::ParsingContext::new(target_ty, &db);
    let annots = TypeAnnotations::default();
    let target = TyWithMeta::new(target_ty, &annots);
    let result = TyResolvedRef::coerce(&ctx, target, &parsed);
    assert!(result.is_ok(), "Failed to parse: {:?}", result);
    let value = result.unwrap().unwrap();
    let json_value = serde_json::to_value(&value).unwrap();
    let expected = serde_json::json!({"key": "value with space", "array": [1, 2, 3], "object": {"key": "value"}});
    assert_eq!(json_value, expected);
}

#[test]
fn test_json_with_unquoted_values_with_spaces_and_new_lines() {
    let db = crate::baml_db! {
        class Foo { key: string }
        class Test { key: string, array: [(int | string)], object: Foo }
    };

    let raw = r#"
  {
    key: "test a long
thing with new

lines",
    array: [1, 2, 3],
    object: {
      key: value
    }
  }
  "#;
    let target_ty = db.resolved_from_ident(&"Test").unwrap();
    let parsed = crate::jsonish::parse(raw, Default::default(), true).unwrap();
    let ctx = crate::deserializer::coercer::ParsingContext::new(target_ty, &db);
    let annots = TypeAnnotations::default();
    let target = TyWithMeta::new(target_ty, &annots);
    let result = TyResolvedRef::coerce(&ctx, target, &parsed);
    assert!(result.is_ok(), "Failed to parse: {:?}", result);
    let value = result.unwrap().unwrap();
    let json_value = serde_json::to_value(&value).unwrap();
    let expected = serde_json::json!({"key": "test a long\nthing with new\n\nlines", "array": [1, 2, 3], "object": {"key": "value"}});
    assert_eq!(json_value, expected);
}

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
    class_ty("Test", vec![
        field("my_field_0", bool_ty()),
        field("my_field_1", string_ty()),
    ]),
    empty_db(),
    {
      "my_field_0": true,
      "my_field_1": "**First fragment, Another fragment**\n\nFrag 2, frag 3. Frag 4, Frag 5, Frag 5.\n\nFrag 6, the rest, of the sentence. Then i would quote something \"like this\" or this.\n\nThen would add a summary of sorts."
    }
);

// --- Whitespace in keys ---

test_deserializer!(
    test_whitespace_in_keys_preserved,
    r#"{" answer ": {" content ": 78.54}}"#,
    string_ty(),
    empty_db(),
    r#"{" answer ": {" content ": 78.54}}"#
);

#[test]
fn test_class_with_whitespace_keys() {
    let db = crate::baml_db! {
        class Answer { content: float }
        class Test { answer: Answer }
    };

    let raw = r#"{" answer ": {" content ": 78.54}}"#;
    let target_ty = db.resolved_from_ident(&"Test").unwrap();
    let parsed = crate::jsonish::parse(raw, Default::default(), true).unwrap();
    let ctx = crate::deserializer::coercer::ParsingContext::new(target_ty, &db);
    let annots = TypeAnnotations::default();
    let target = TyWithMeta::new(target_ty, &annots);
    let result = TyResolvedRef::coerce(&ctx, target, &parsed);
    assert!(result.is_ok(), "Failed to parse: {:?}", result);
    let value = result.unwrap().unwrap();
    let json_value = serde_json::to_value(&value).unwrap();
    let expected = serde_json::json!({"answer": {"content": 78.54}});
    assert_eq!(json_value, expected);
}

// --- Malformed JSON sequence (partial test) ---

#[test]
fn test_mal_formed_json_sequence() {
    let db = crate::baml_db! {
        class Foo1 { field1: string, field2: string, field3: string, field4: string, field5: string, field6: string }
        class Foo2 { field7: string, field8: string, field9: string, field10: string, field11: string, field12: string, field13: string, field14: string, field15: string, field16: string, field17: string, field18: string, field19: string, field20: string, field21: string, field22: string, field23: string, field24: string, field25: string }
        class Foo3 { field28: string, field29: [string], field30: [string], field31: [string], field32: [string], field33: string, field34: string, field35: string, field36: string }
        class Test { foo1: Foo1, foo2: [Foo2], foo3: Foo3 }
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
    let target_ty = db.resolved_from_ident(&"Test").unwrap();
    let parsed = crate::jsonish::parse(raw, Default::default(), false).unwrap();
    let ctx = crate::deserializer::coercer::ParsingContext::new(target_ty, &db);
    let annots = TypeAnnotations::default();
    let target = TyWithMeta::new(target_ty, &annots);
    let result = TyResolvedRef::coerce(&ctx, target, &parsed);
    assert!(result.is_ok(), "Failed to parse: {:?}", result);
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
    array_of(annotated(class_ty("Test", vec![
        field("id", string_ty()),
        field("English", string_ty()),
        field("Portuguese", string_ty()),
    ]))),
    empty_db(),
    [{
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
      }]
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
    array_of(annotated(class_ty("Test", vec![
        field("id", string_ty()),
        field("English", string_ty()),
        optional_field("Portuguese", string_ty()),
    ]))),
    empty_db(),
    [{
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
      }]
);

// --- SIDD test ---

#[test]
fn test_sidd() {
    let db = crate::baml_db! {
        class Heading { heading: string, python_function_code: string, description: string }
        class Headings { headings: [Heading] }
    };

    let raw = r#"
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
  "#
    .trim();

    let target_ty = db.resolved_from_ident(&"Headings").unwrap();
    let parsed = crate::jsonish::parse(raw, Default::default(), true).unwrap();
    let ctx = crate::deserializer::coercer::ParsingContext::new(target_ty, &db);
    let annots = TypeAnnotations::default();
    let target = TyWithMeta::new(target_ty, &annots);
    let result = TyResolvedRef::coerce(&ctx, target, &parsed);
    assert!(result.is_ok(), "Failed to parse: {:?}", result);
    let value = result.unwrap().unwrap();
    let json_value = serde_json::to_value(&value).unwrap();
    let expected = serde_json::json!({
      "headings": [
        {
          "heading": "Urban Oasis",
          "python_function_code": "def is_urban_oasis(property):\n       return 'Large Green Area' in property['amenities'] or 'Garden' in property['amenities']",
          "description": "Properties that offer a serene living experience amidst the bustling city life."
        }
      ]
    });
    assert_eq!(json_value, expected);
}

#[test]
fn test_injected_triple_quoted_string() {
    let db = crate::baml_db! {
        class Heading { heading: string, python_function_code: string, description: string }
        class Headings { headings: [Heading] }
    };

    let raw = r#"
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
  "#
    .trim();

    let target_ty = db.resolved_from_ident(&"Headings").unwrap();
    let parsed = crate::jsonish::parse(raw, Default::default(), true).unwrap();
    let ctx = crate::deserializer::coercer::ParsingContext::new(target_ty, &db);
    let annots = TypeAnnotations::default();
    let target = TyWithMeta::new(target_ty, &annots);
    let result = TyResolvedRef::coerce(&ctx, target, &parsed);
    assert!(result.is_ok(), "Failed to parse: {:?}", result);
    let value = result.unwrap().unwrap();
    let json_value = serde_json::to_value(&value).unwrap();
    let expected = serde_json::json!({
      "headings": [
        {
          "heading": "Urban Oasis",
          "python_function_code": "def is_urban_oasis(property):\n       return 'Large Green Area' in property['amenities'] or 'Garden' in property['amenities']",
          "description": "Properties that offer a serene living experience amidst the bustling city life."
        }
      ]
    });
    assert_eq!(json_value, expected);
}
