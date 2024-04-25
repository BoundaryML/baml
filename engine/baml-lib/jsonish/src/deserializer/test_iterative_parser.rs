use super::iterative_parser;

macro_rules! test_jsonish {
    ($test_name:ident, $raw_str:expr, $($json:tt)+) => {
        #[test]
        fn $test_name() {
            let raw_str = $raw_str;
            let val = iterative_parser::parse_jsonish_value(raw_str, iterative_parser::JSONishOptions::default());
            assert!(val.is_ok(), "Failed to parse: {:?}\n{}", val, raw_str);

            // Using serde_json::json! macro to parse the expected JSON string into a serde_json::Value
            let expected_val = serde_json::json!($($json)+);

            assert_eq!(val.unwrap(), expected_val);
        }
    };
}

test_jsonish!(test_null, "null", null);

test_jsonish!(test_number, "12111", 12111);

test_jsonish!(test_string, r#""hello""#, "hello");

test_jsonish!(test_bool, "true", true);

test_jsonish!(test_array, "[1, 2, 3]", [1, 2, 3]);

test_jsonish!(test_object, r#"{"key": "value"}"#, {"key": "value"});

test_jsonish!(test_nested, r#"{"key": [1, 2, 3]}"#, {"key": [1, 2, 3]});

// now with whitespace
test_jsonish!(test_nested_whitespace, r#" { "key" : [ 1 , 2 , 3 ] } "#, {"key": [1, 2, 3]});

// Now with leading and suffix text.
test_jsonish!(test_nested_whitespace_prefix_suffix, r#"prefix { "key" : [ 1 , 2 , 3 ] } suffix"#, {"key": ["1", "2", "3"]});

// Now with multiple top level objects
test_jsonish!(test_multiple_top_level, r#"{"key": "value"} {"key": "value"}"#, [{"key": "value"}, {"key": "value"}]);

// With prefix and suffix
test_jsonish!(test_multiple_top_level_prefix_suffix, r#"prefix {"key": "value"} some random text {"key": "value"} suffix"#, [{"key": "value"}, {"key": "value"}]);

// Trailing comma
// The jsonish parser will return the value as a string as we do our best not to cast or modify the input when types are not clear.
test_jsonish!(test_trailing_comma_array, r#"[1, 2, 3,]"#, ["1", "2", "3"]);
test_jsonish!(test_trailing_comma_object, r#"{"key": "value",}"#, {"key": "value"});

// Test cases for invalid JSONish
test_jsonish!(test_invalid_array, "[1, 2, 3", ["1", "2", "3"]);

test_jsonish!(test_invalid_array_in_object, r#"{"key": [1, 2, 3"#, {"key": ["1", "2", "3"]});

// Extra quote is not allowed
test_jsonish!(test_incomplete_string, r#""hello"#, "hello");

test_jsonish!(test_incomplete_string_in_object, r#"{"key": "value"#, {"key": "value"});

// This is un-changed
test_jsonish!(
    test_prefixed_incompleted_string,
    r#"prefix "hello"#,
    r#"prefix "hello"#
);

test_jsonish!(
  test_large_object,
  r#"{"key": "value", "array": [1, 2, 3], "object": {"key": "value"}}"#,
  {
    "key": "value",
    "array": [1, 2, 3],
    "object": {
      "key": "value"
    }
  }
);

test_jsonish!(
  test_json_md_example,
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
  {
    "key": "value",
    "array": [1, 2, 3],
    "object": {
      "key": "value"
    }
  }
);

test_jsonish!(
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
  [{
    "key": "value",
    "array": [1, 2, 3],
    "object": {
      "key": "value"
    }
  }, ["1", "2"]]
);

test_jsonish!(
  test_json_md_bad_example,
  r#"
  some text
  ```json
  {
    "key": "value",
    "array": [1, 2, 3,],
    "object": {
      "key": "value"
    },
  }
  ```
  "#,
  {
    "key": "value",
    "array": ["1", "2", "3"],
    "object": {
      "key": "value"
    }
  }
);

test_jsonish!(
  test_json_md_bad_example_2,
  r#"
  some text
  ```json
  {
    "key": "value",
    "array": [1, 2, 3, "some stinrg"with quotes"],
    "object": {
      "key": "value"
    },
  }
  ```
  "#,
  {
    "key": "value",
    "array": ["1", "2", "3", "some stinrg\"with quotes"],
    "object": {
      "key": "value"
    }
  }
);

test_jsonish!(
  test_json_md_bad_example_single_quote,
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
  {
    "key": "value",
    "array": ["1", "2", "3", "some stinrg'   with quotes"],
    "object": {
      "key": "value"
    }
  }
);

test_jsonish!(
  test_json_with_unquoted_keys,
  r#"
  {
    key: "value",
    array: [1, 2, 3],
    object: {
      key: "value"
    }
  }
  "#,
  {
    "key": "value",
    "array": ["1", "2", "3"],
    "object": {
      "key": "value"
    }
  }
);

test_jsonish!(
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
  {
    "key": "value with space",
    "array": ["1", "2", "3"],
    "object": {
      "key": "value"
    }
  }
);

test_jsonish!(
  test_json_with_string_with_new_line,
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
  {
    "key": "test a long\nthing with new\n\nlines",
    "array": ["1", "2", "3"],
    "object": {
      "key": "value"
    }
  }
);

test_jsonish!(
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
    {
      "my_field_0": "true",
      "my_field_1": "**First fragment, Another fragment**\n\nFrag 2, frag 3. Frag 4, Frag 5, Frag 5.\n\nFrag 6, the rest, of the sentence. Then i would quote something \"like this\" or this.\n\nThen would add a summary of sorts."
    }
);
