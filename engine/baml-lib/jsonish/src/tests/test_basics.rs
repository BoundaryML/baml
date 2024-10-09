use super::*;

test_deserializer!(test_null, EMPTY_FILE, "null", FieldType::null(), null);
test_deserializer!(
    test_null_1,
    EMPTY_FILE,
    "null",
    FieldType::string().as_optional(),
    null
);
test_deserializer!(
    test_null_2,
    EMPTY_FILE,
    "Null",
    FieldType::string().as_optional(),
    // This is a string, not null
    "Null"
);

test_deserializer!(
    test_null_3,
    EMPTY_FILE,
    "None",
    FieldType::string().as_optional(),
    // This is a string, not null
    "None"
);

test_deserializer!(test_number, EMPTY_FILE, "12111", FieldType::int(), 12111);
test_deserializer!(test_number_2, EMPTY_FILE, "12,111", FieldType::int(), 12111);
test_deserializer!(
    test_string,
    EMPTY_FILE,
    r#""hello""#,
    FieldType::string(),
    "\"hello\""
);

test_deserializer!(test_bool, EMPTY_FILE, "true", FieldType::bool(), true);
test_deserializer!(test_bool_2, EMPTY_FILE, "True", FieldType::bool(), true);
test_deserializer!(test_bool_3, EMPTY_FILE, "false", FieldType::bool(), false);
test_deserializer!(test_bool_4, EMPTY_FILE, "False", FieldType::bool(), false);

test_deserializer!(
    test_float,
    EMPTY_FILE,
    "12111.123",
    FieldType::float(),
    12111.123
);

test_deserializer!(
    test_float_comma_us,
    EMPTY_FILE,
    "12,111.123",
    FieldType::float(),
    12111.123
);

// uncomment when we support european formatting.
// test_deserializer!(
//     test_float_comma_german,
//     EMPTY_FILE,
//     "12.111,123",
//     FieldType::float(),

// );

test_deserializer!(
    test_float_comma_german2,
    EMPTY_FILE,
    "12.11.",
    FieldType::float(),
    12.11
);

test_deserializer!(test_float_1, EMPTY_FILE, "1/5", FieldType::float(), 0.2);

test_deserializer!(
    test_array,
    EMPTY_FILE,
    r#"[1, 2, 3]"#,
    FieldType::int().as_list(),
    [1, 2, 3]
);

test_deserializer!(
    test_array_1,
    EMPTY_FILE,
    r#"[1, 2, 3]"#,
    FieldType::string().as_list(),
    ["1", "2", "3"]
);

test_deserializer!(
    test_array_3,
    EMPTY_FILE,
    r#"[1, 2, 3]"#,
    FieldType::float().as_list(),
    [1., 2., 3.]
);

test_deserializer!(
    test_object,
    r#"
    class Test {
        key string
    }
    "#,
    r#"{"key": "value"}"#,
    FieldType::class("Test"),
    {"key": "value"}
);

test_deserializer!(
  test_nested,
    r#"
    class Test {
        key int[]
    }
    "#,
    r#"{"key": [1, 2, 3]}"#,
    FieldType::class("Test"),
    {"key":  [1, 2, 3]}
);

// now with whitespace
test_deserializer!(
  test_nested_whitespace,
    r#"
    class Test {
        key int[]
    }
    "#,
    r#" { "key" : [ 1 , 2 , 3 ] } "#,
    FieldType::class("Test"),
    {"key":  [1, 2, 3]}
);

// Now with leading and suffix text.
test_deserializer!(
  test_nested_whitespace_prefix_suffix,
  r#"
  class Test {
      key int[]
  }
  "#,
    r#"prefix { "key" : [ 1 , 2 , 3 ] } suffix"#,
    FieldType::class("Test"),
    {"key":  [1, 2, 3]}
);

// Now with multiple top level objects
test_deserializer!(
  test_multiple_top_level_1,
  r#"
  class Test {
      key string
  }
  "#,
  r#"{"key": "value1"} {"key": "value2"}"#,
  FieldType::class("Test"),
  {"key": "value1"}
);

test_deserializer!(
  test_multiple_top_level_2,
  r#"
  class Test {
      key string
  }
  "#,
  r#"{"key": "value1"} {"key": "value2"}"#,
  FieldType::class("Test").as_list(),
  [{"key": "value1"}, {"key": "value2"}]
);

// With prefix and suffix
test_deserializer!(
  test_multiple_top_level_prefix_suffix_1,
  r#"
  class Test {
      key string
  }
  "#,
  r#"prefix {"key": "value1"} some random text {"key": "value2"} suffix"#,
  FieldType::class("Test"),
  {"key": "value1"}
);

test_deserializer!(
  test_multiple_top_level_prefix_suffix_2,
  r#"
  class Test {
      key string
  }
  "#,
  r#"prefix {"key": "value1"} some random text {"key": "value2"} suffix"#,
  FieldType::class("Test").as_list(),
  [{"key": "value1"}, {"key": "value2"}]
);

// Trailing comma
// The jsonish parser will return the value as a string as we do our best not to cast or modify the input when types are not clear.
test_deserializer!(
    test_trailing_comma_array_2,
    EMPTY_FILE,
    r#"[1, 2, 3,]"#,
    FieldType::int().as_list(),
    [1, 2, 3]
);

test_deserializer!(
    test_trailing_comma_array_3,
    EMPTY_FILE,
    r#"[1, 2, 3,]"#,
    FieldType::string().as_list(),
    ["1", "2", "3"]
);

test_deserializer!(
    test_trailing_comma_object,
    r#"
    class Test {
        key string
    }
    "#,
    r#"{"key": "value",}"#,
    FieldType::class("Test"),
    {"key": "value"}
);

// Test cases for invalid JSONish
test_deserializer!(
    test_invalid_array,
    EMPTY_FILE,
    r#"[1, 2, 3"#,
    FieldType::int().as_list(),
    [1, 2, 3]
);

test_deserializer!(
  test_invalid_array_in_object,
  r#"
  class Test {
      key int[]
  }
  "#,
  r#"{"key": [1, 2, 3"#,
  FieldType::class("Test"),
  {"key": [1, 2, 3]}
);

// Extra quote is not allowed
test_deserializer!(
    test_incomplete_string,
    EMPTY_FILE,
    r#""hello"#,
    FieldType::string(),
    "\"hello"
);

test_deserializer!(
    test_incomplete_string_in_object,
    r#"
    class Test {
        key string
    }
    "#,
    r#"{"key": "value"#,
    FieldType::class("Test"),
    {"key": "value"}
);

// This is un-changed
test_deserializer!(
    test_prefixed_incompleted_string,
    EMPTY_FILE,
    r#"prefix "hello"#,
    FieldType::string(),
    "prefix \"hello"
);

test_deserializer!(
   test_large_object,
    r#"
    class Test {
        key string
        array int[]
        object Foo
    }

    class Foo {
        key string
    }
    "#,
    r#"{"key": "value", "array": [1, 2, 3], "object": {"key": "value"}}"#,
    FieldType::class("Test"),
    {"key": "value", "array": [1, 2, 3], "object": {"key": "value"}}
);

test_deserializer!(
  test_json_md_example_1,
  r#"
  class Test {
      key string
      array int[]
      object Foo
  }

  class Foo {
      key string
  }
  "#,
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
  FieldType::class("Test"),
  {"key": "value", "array": [1, 2, 3], "object": {"key": "value"}}
);

test_deserializer!(
  test_json_md_example_2,
  r#"
  class Test {
      key string
      array int[]
      object Foo
  }

  class Foo {
      key string
  }
  "#,
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
  FieldType::class("Test"),
  {"key": "value", "array": [1, 2, 3], "object": {"key": "value"}}
);

test_deserializer!(
    test_json_md_example_3,
    r#"
  class Test {
      key string
      array int[]
      object Foo
  }

  class Foo {
      key string
  }
  "#,
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
    FieldType::int().as_list(),
    [1, 2]
);

test_deserializer!(
  test_json_md_example_1_bad_inner_json,
  r#"
  class Test {
      key string
      array int[]
      object Foo
  }

  class Foo {
      key string
  }
  "#,
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
  FieldType::class("Test"),
  {"key": "value", "array": [1, 2, 3], "object": {"key": "value"}}
);

test_deserializer!(
  test_json_md_example_1_bad_inner_json_2,
  r#"
  class Test {
      key string
      array (int | string)[]
      object Foo
  }

  class Foo {
      key string
  }
  "#,
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
  FieldType::class("Test"),
  {"key": "value", "array": [1, 2, 3, "somet\"string with quotes"], "object": {"key": "value"}}
);

test_deserializer!(
  test_json_md_example_1_bad_inner_json_3,
  r#"
  class Test {
      key string
      array (int | string)[]
      object Foo
  }

  class Foo {
      key string
  }
  "#,
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
  FieldType::class("Test"),
  {"key": "value", "array": [1, 2, 3, "some stinrg'   with quotes"], "object": {"key": "value"}}
);

test_deserializer!(
  test_unquoted_keys,
  r#"
  class Test {
      key string
      array (int | string)[]
      object Foo
  }

  class Foo {
      key string
  }
  "#,
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
  FieldType::class("Test"),
  {"key": "value", "array": [1, 2, 3, "some stinrg'   with quotes"], "object": {"key": "value"}}
);

test_deserializer!(
  test_json_with_unquoted_values_with_spaces,
  r#"
  class Test {
      key string
      array (int | string)[]
      object Foo
  }

  class Foo {
      key string
  }
  "#,
  r#"
  {
    key: value with space,
    array: [1, 2, 3],
    object: {
      key: value
    }
  }
  "#,
  FieldType::class("Test"),
  {"key": "value with space", "array": [1, 2, 3], "object": {"key": "value"}}
);

test_deserializer!(
  test_json_with_unquoted_values_with_spaces_and_new_lines,
  r#"
  class Test {
      key string
      array (int | string)[]
      object Foo
  }

  class Foo {
      key string
  }
  "#,
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
  FieldType::class("Test"),
  {"key": "test a long\nthing with new\n\nlines", "array": [1, 2, 3], "object": {"key": "value"}}
);

test_deserializer!(
  test_json_with_markdown_without_quotes,
  r#"
  class Test {
      my_field_0 bool
      my_field_1 string
  }
  "#,
    r#"
  {
    "my_field_0": true,
    "my_field_1": **First fragment, Another fragment**

Frag 2, frag 3. Frag 4, Frag 5, Frag 5.

Frag 6, the rest, of the sentence. Then i would quote something "like this" or this.

Then would add a summary of sorts.
  }
  "#,
    FieldType::class("Test"),
    {
      "my_field_0": true,
      "my_field_1": "**First fragment, Another fragment**\n\nFrag 2, frag 3. Frag 4, Frag 5, Frag 5.\n\nFrag 6, the rest, of the sentence. Then i would quote something \"like this\" or this.\n\nThen would add a summary of sorts."
    }
);

test_partial_deserializer!(
  test_mal_formed_json_sequence,
  r#"
  class Test {
    foo1 Foo1
    foo2 Foo2[]
    foo3 Foo3
  }

  class Foo1 {
    field1 string
    field2 string
    field3 string
    field4 string
    field5 string
    field6 string
  }

  class Foo2 {
    field7 string
    field8 string
    field9 string
    field10 string
    field11 string
    field12 string
    field13 string
    field14 string
    field15 string
    field16 string
    field17 string
    field18 string
    field19 string
    field20 string
    field21 string
    field22 string
    field23 string
    field24 string
    field25 string
  }

  class Foo3 {
    field28 string
    field29 string[]
    field30 string[]
    field31 string[]
    field32 string[]
    field33 string
    field34 string
    field35 string
    field36 string
  }
  "#,
 r#"```json
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
}"#, 
FieldType::class("Test"),
{
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
      "field25": null,
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

test_deserializer!(
  test_localization,
  r#"
  class Test {
    id string
    English string
    Portuguese string
  }
  "#,
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
  FieldType::class("Test").as_list(),
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
  class Test {
    id string
    English string
    Portuguese string?
  }
  "#,
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
]
  "#.trim(),
  FieldType::class("Test").as_list(),
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

const SIDD_BAML: &str = r#"
class Headings {
    headings Heading[]
}
class Heading {
    heading string
    python_function_code string
    description string
}
"#;

test_deserializer!(
  test_sidd,
  SIDD_BAML,
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
  "#.trim(),
  baml_types::FieldType::Class("Headings".into()),

  {
    "headings": [
      {
        "heading": "Urban Oasis",
        "python_function_code": r#"def is_urban_oasis(property):
       return 'Large Green Area' in property['amenities'] or 'Garden' in property['amenities']"#,
        "description": "Properties that offer a serene living experience amidst the bustling city life."
      }
    ]
  }
);

test_deserializer!(
  test_injected_triple_quoted_string,
  SIDD_BAML,
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
  "#.trim(),
  baml_types::FieldType::Class("Headings".into()),

  {
    "headings": [
      {
        "heading": "Urban Oasis",
        "python_function_code": r#"def is_urban_oasis(property):
       return 'Large Green Area' in property['amenities'] or 'Garden' in property['amenities']"#,
        "description": "Properties that offer a serene living experience amidst the bustling city life."
      }
    ]
  }
);
