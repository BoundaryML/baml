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
test_jsonish!(test_nested_whitespace_prefix_suffix, r#"prefix { "key" : [ 1 , 2 , 3 ] } suffix"#, {"key": [1, 2, 3]});

// Now with multiple top level objects
test_jsonish!(test_multiple_top_level, r#"{"key": "value"} {"key": "value"}"#, [{"key": "value"}, {"key": "value"}]);

// With prefix and suffix
test_jsonish!(test_multiple_top_level_prefix_suffix, r#"prefix {"key": "value"} some random text {"key": "value"} suffix"#, [{"key": "value"}, {"key": "value"}]);

// Trailing comma
// The jsonish parser will return the value as a string as we do our best not to cast or modify the input when types are not clear.
test_jsonish!(test_trailing_comma_array, r#"[1, 2, 3,]"#, [1, 2, 3]);
test_jsonish!(test_trailing_comma_object, r#"{"key": "value",}"#, {"key": "value"});

// Test cases for invalid JSONish
test_jsonish!(test_invalid_array, "[1, 2, 3", [1, 2, 3]);

test_jsonish!(test_invalid_array_in_object, r#"{"key": [1, 2, 3"#, {"key": [1, 2, 3]});

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
    "array": [1, 2, 3],
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
    "array": [1, 2, 3, "some stinrg\"with quotes"],
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
    "array": [1, 2, 3, "some stinrg'   with quotes"],
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
    "array": [1, 2, 3],
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
    "array": [1, 2, 3],
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
    "array": [1, 2, 3],
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
      "my_field_0": true,
      "my_field_1": "**First fragment, Another fragment**\n\nFrag 2, frag 3. Frag 4, Frag 5, Frag 5.\n\nFrag 6, the rest, of the sentence. Then i would quote something \"like this\" or this.\n\nThen would add a summary of sorts."
    }
);

test_jsonish!(test_mal_formed_json_sequence, r#"```json
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
}"#, {
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
});

test_jsonish!(
  test_paulo,
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
```
  "#.trim(),
  [
    {},
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
  ]
);
