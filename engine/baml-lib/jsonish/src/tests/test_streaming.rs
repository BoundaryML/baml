use baml_types::ir_type::UnionConstructor;

use super::*;
use crate::helpers::load_test_ir;

const NUMBERS: &str = r#"
class Foo {
  nums int[]
}
"#;

test_partial_deserializer_streaming!(
    test_number_list,
    NUMBERS,
    "{'nums': [1,2",
    TypeIR::class("Foo"),
    {"nums": [1]}
);

const NUMBERS_STATE: &str = r#"
class Foo {
  nums int[] @stream.with_state
  bar int @stream.with_state
}
"#;

test_partial_deserializer_streaming!(
    test_number_list_state_incomplete,
    NUMBERS_STATE,
    "{'nums': [1,2",
    TypeIR::class("Foo"),
    {"nums": {"value": [1], "state": "Incomplete"}, "bar": {"value": null, "state": "Pending"}}
);

const TOPLEVEL_DONE: &str = r#"
class Foo {
  nums int[]
  @@stream.done
}
"#;

test_partial_deserializer_streaming_failure!(
    test_toplevel_done,
    TOPLEVEL_DONE,
    "{'nums': [1,2]",
    {
        let mut class = TypeIR::class("Foo");
        class.meta_mut().streaming_behavior.needed = true;
        class
    }
);

const NESTED_DONE: &str = r#"
class Foo {
  nums int[]
  @@stream.done
}

class Bar {
  foos Foo[]
}
"#;

test_partial_deserializer_streaming!(
  test_nested_done,
  NESTED_DONE,
  r#"{
    'foos': [
      {'nums': [1, 2]},
      {'nums': [3, 4]
  "#,
  TypeIR::class("Bar"),
  {"foos": [ {"nums": [1, 2]}]}
);

const NESTED_DONE_WITH_TOPLEVEL_DONE: &str = r#"
class Foo {
  nums int[]
  @@stream.done
}

class Bar {
  message string @stream.done
  foos Foo[]
}
"#;

test_partial_deserializer_streaming!(
  test_nested_done_with_toplevel_done,
  NESTED_DONE_WITH_TOPLEVEL_DONE,
  r#"{
    'message': "Hello",
    'foos': [
      {'nums': [1, 2]},
      {'nums': [3, 4]
  "#,
  TypeIR::class("Bar"),
  {"message": "Hello", "foos": [ {"nums": [1, 2]}]}
);

const NEEDED_FIELD: &str = r#"
class Foo {
  my_int int
  my_string string @stream.not_null
}

class Bar {
  foos Foo[]

  @@stream.not_null
}
"#;

test_partial_deserializer_streaming!(
  test_needed_field,
  NEEDED_FIELD,
  // r#"{"foos": [{"my_int": 1, "my_string": "hi"}, {"my_int": 10,"#,
  r#"{"foos": [{"my_int": 1, "my"#,
  TypeIR::class("Bar"),
  {"foos": []}
);

const DONE_FIELD: &str = r#"
class Foo {
  foo string @stream.done
  bar string

  @@stream.not_null
}
"#;

test_partial_deserializer_streaming!(
  test_done_field_0,
  DONE_FIELD,
  r#"{"foo": ""#,
  TypeIR::class("Foo"),
  {"foo": null, "bar": null}
);

test_partial_deserializer_streaming!(
  test_done_field_1,
  DONE_FIELD,
  r#"{"foo": """#,
  TypeIR::class("Foo"),
  {"foo": "", "bar": null}
);

const MEMORY_TEST: &str = r##"
class MemoryObject {
  id string
  name string
  description string
}

class ComplexMemoryObject {
  id string
  name string
  description string
  metadata (string | int | float)[] @description(#"
    Additional metadata about the memory object, which can be a mix of types.
  "#)
}

class AnotherObject {
  id string
  thingy2 string
  thingy3 string
}

class TestMemoryOutput {
  items (MemoryObject | ComplexMemoryObject | AnotherObject)[] @description(#"
    Add 10 items, which can be either simple MemoryObjects or more complex MemoryObjects with metadata.
  "#)
  more_items (MemoryObject | ComplexMemoryObject | AnotherObject)[] @description(#"
    Add 3 more items, which can be either simple MemoryObjects or more complex MemoryObjects with metadata.
  "#)
}
"##;

const MEMORY_PAYLOAD: &str = r#"
{
  "items": [
    {
      "id": "1",
      "name": "MemoryObject1",
      "description": "A simple memory object."
    },
    {
      "id": "2",
      "name": "MemoryObject2",
      "description": "A more complex memory object with metadata.",
      "metadata": [
        "metadata1",
        42,
        3.12
      ]
    },
    {
      "id": "3",
      "thingy2": "Thingy2Value",
      "thingy3": "Thingy3Value"
    },
    {
      "id": "4",
      "name": "MemoryObject4",
      "description": "Another simple memory object."
    },
    {
      "id": "5",
      "name": "MemoryObject5",
      "description": "Complex object with metadata.",
      "metadata": [
        "additional info",
        100,
        2.715
      ]
    },
    {
      "id": "6",
      "thingy2": "AnotherThingy2",
      "thingy3": "AnotherThingy3"
    },
    {
      "id": "7",
      "name": "MemoryObject7",
      "description": "Simple object with no metadata."
    },
    {
      "id": "8",
      "name": "MemoryObject8",
      "description": "Complex object with varied metadata.",
      "metadata": [
        "info",
        256,
        1.618
      ]
    },
    {
      "id": "9",
      "thingy2": "Thingy2Example",
      "thingy3": "Thingy3Example"
    },
    {
      "id": "10",
      "name": "MemoryObject10",
      "description": "Final simple memory object."
    }
  ],
  "more_items": [
    {
      "id": "11",
      "name": "MemoryObject11",
      "description": "Additional simple memory object."
    },
    {
      "id": "12",
      "name": "MemoryObject12",
      "description": "Additional complex object with metadata.",
      "metadata": [
        "extra data",
        512,
        0.577
      ]
    },
    {
      "id": "13",
      "thingy2": "ExtraThingy2",
      "thingy3": "ExtraThingy3"
    }
  ]
}
"#;

test_partial_deserializer_streaming!(
    test_memory,
    MEMORY_TEST,
    MEMORY_PAYLOAD,
    TypeIR::class("TestMemoryOutput"),
    {
      "items": [
        {
          "id": "1",
          "name": "MemoryObject1",
          "description": "A simple memory object."
        },
        {
          "id": "2",
          "name": "MemoryObject2",
          "description": "A more complex memory object with metadata.",
          "metadata": [
            "metadata1",
            42,
            3.12
          ]
        },
        {
          "id": "3",
          "thingy2": "Thingy2Value",
          "thingy3": "Thingy3Value"
        },
        {
          "id": "4",
          "name": "MemoryObject4",
          "description": "Another simple memory object."
        },
        {
          "id": "5",
          "name": "MemoryObject5",
          "description": "Complex object with metadata.",
          "metadata": [
            "additional info",
            100,
            2.715
          ]
        },
        {
          "id": "6",
          "thingy2": "AnotherThingy2",
          "thingy3": "AnotherThingy3"
        },
        {
          "id": "7",
          "name": "MemoryObject7",
          "description": "Simple object with no metadata."
        },
        {
          "id": "8",
          "name": "MemoryObject8",
          "description": "Complex object with varied metadata.",
          "metadata": [
            "info",
            256,
            1.618
          ]
        },
        {
          "id": "9",
          "thingy2": "Thingy2Example",
          "thingy3": "Thingy3Example"
        },
        {
          "id": "10",
          "name": "MemoryObject10",
          "description": "Final simple memory object."
        }
      ],
      "more_items": [
        {
          "id": "11",
          "name": "MemoryObject11",
          "description": "Additional simple memory object."
        },
        {
          "id": "12",
          "name": "MemoryObject12",
          "description": "Additional complex object with metadata.",
          "metadata": [
            "extra data",
            512,
            0.577
          ]
        },
        {
          "id": "13",
          "thingy2": "ExtraThingy2",
          "thingy3": "ExtraThingy3"
        }
      ]
    }
);

const TODO_TOOLS_EXAMPLE: &str = r#"
type Tool = MessageToUser | AddItem | AdjustItem | GetLastItemId

class MessageToUser {
    type "message_to_user" @stream.not_null
    message string @stream.not_null
}

class AdjustItem {
    type "adjust_item" @stream.not_null
    item_id int
    title string?
    @@stream.done
}

class AddItem {
    type "add_item" @stream.not_null
    title string @stream.not_null
    @@stream.done
}

class GetLastItemId {
    type "get_last_item_id" @stream.not_null
    @@stream.done
}
"#;

test_partial_deserializer_streaming!(
    test_todo_tools_message,
    TODO_TOOLS_EXAMPLE,
    r#"{"type": "message_to_user", "message": "Hello us"#,
    TypeIR::union(vec![
        TypeIR::class("MessageToUser"),
        TypeIR::class("AdjustItem"),
        TypeIR::class("AddItem"),
        TypeIR::class("GetLastItemId"),
    ]),
    {
        "type": "message_to_user",
        "message": "Hello us"
    }
);

test_partial_deserializer_streaming_failure!(
    test_todo_tools_adjust_item,
    TODO_TOOLS_EXAMPLE,
    r#"{"type": "adjust_item", "item_id": 1, "title": "New Title"#,
    {
        let mut union = TypeIR::union(vec![
            TypeIR::class("MessageToUser"),
            TypeIR::class("AdjustItem"),
            TypeIR::class("AddItem"),
            TypeIR::class("GetLastItemId"),
        ]);
        union.meta_mut().streaming_behavior.needed = true;
        union
    }
);

// Test for @stream.not_null fields receiving null values during streaming
const STREAM_NOT_NULL_TEST: &str = r#"
class ClassWithBlockDone {
    i_16_digits int
    s_20_words string
    @@stream.done
}

class ClassWithoutDone {
    i_16_digits int
    s_20_words string @description("A string with 20 words in it") @stream.with_state
}

class SemanticContainer {
    sixteen_digit_number int
    string_with_twenty_words string @stream.done
    class_1 ClassWithoutDone
    class_2 ClassWithBlockDone
    class_done_needed ClassWithBlockDone @stream.not_null
    class_needed ClassWithoutDone @stream.not_null
    three_small_things SmallThing[] @description("Should have three items.")
    final_string string
}

class SmallThing {
    i_16_digits int @stream.not_null
    i_8_digits int
}
"#;

// This test simulates the scenario where @stream.not_null fields are null
// during partial streaming, which should cause validation to fail
test_partial_deserializer_streaming_failure!(
    test_stream_not_null_with_partial_data,
    STREAM_NOT_NULL_TEST,
    r#"{
        "sixteen_digit_number": 1234567890123456,
        "string_with_twenty_words": "This is a string with exactly twenty words in it for testing purposes and validation",
        "class_1": null,
        "class_2": null,
        "class_done_needed": null,
        "class_needed": null,
        "three_small_things": [],
        "final_string": "end"
    }"#,
    {
        let mut class = TypeIR::class("SemanticContainer");
        class.meta_mut().streaming_behavior.needed = true;
        class
    }
);

// This test shows that when @stream.not_null fields have values, parsing succeeds
test_partial_deserializer_streaming!(
    test_stream_not_null_with_complete_data,
    STREAM_NOT_NULL_TEST,
    r#"{
        "sixteen_digit_number": 12345678,
        "string_with_twenty_words": "This is a string with exactly twenty words in it for testing purposes and validation",
        "class_1": {
            "i_16_digits": 12345678,
            "s_20_words": "Another string with twenty words"
        },
        "class_2": {
            "i_16_digits": 98765432,
            "s_20_words": "Yet another string here"
        },
        "class_done_needed": {
            "i_16_digits": 11111111,
            "s_20_words": "Required class string"
        },
        "class_needed": {
            "i_16_digits": 22222222,
            "s_20_words": "Another required string"
        },
        "three_small_things": [
            {"i_16_digits": 33333333, "i_8_digits": 12345678}
        ],
        "final_string": "end"
    }"#,
    TypeIR::class("SemanticContainer"),
    {
        "sixteen_digit_number": 12345678,
        "string_with_twenty_words": "This is a string with exactly twenty words in it for testing purposes and validation",
        "class_1": {
            "i_16_digits": 12345678,
            "s_20_words": {"value": "Another string with twenty words", "state": "Complete"}
        },
        "class_2": {
            "i_16_digits": 98765432,
            "s_20_words": "Yet another string here"
        },
        "class_done_needed": {
            "i_16_digits": 11111111,
            "s_20_words": "Required class string"
        },
        "class_needed": {
            "i_16_digits": 22222222,
            "s_20_words": {"value": "Another required string", "state": "Complete"}
        },
        "three_small_things": [
            {"i_16_digits": 33333333, "i_8_digits": 12345678}
        ],
        "final_string": "end"
    }
);

// Test for union types with @stream.not_null
const UNION_NOT_NULL_TEST: &str = r#"
class Foo {
  y (string | null) @stream.not_null
}
"#;

// This test should not fail because y is null but marked as @stream.not_null (the @stream.not_null is ignored)
test_partial_deserializer!(
    test_union_not_null_with_null_value,
    UNION_NOT_NULL_TEST,
    r#"{"y": null}"#,
    {
        let mut class = TypeIR::class("Foo");
        class.meta_mut().streaming_behavior.needed = true;
        class
    },
    {"y": null}
);

// This test should succeed because y has a non-null value
test_partial_deserializer_streaming!(
    test_union_not_null_with_string_value,
    UNION_NOT_NULL_TEST,
    r#"{"y": "hello"}"#,
    TypeIR::class("Foo"),
    {"y": "hello"}
);

// Regression test for GitHub issue #2567: Streaming Bug with Parsing Artifacts
// When streaming with partial data, AnyOf values should extract the original
// string content, not leak internal parsing representations like "AnyOf[..."
const STREAMING_ANYOF_BUG: &str = r#"
class Inspiration {
  Description string
}
"#;

test_partial_deserializer_streaming!(
    test_streaming_anyof_string_field,
    STREAMING_ANYOF_BUG,
    r#"{"Description": "A beautiful sunset over the ocean"#,
    TypeIR::class("Inspiration"),
    {"Description": "A beautiful sunset over the ocean"}
);

// Test with a more complex partial input that might trigger AnyOf creation
// This simulates the scenario where the parser is uncertain about structure
test_partial_deserializer_streaming!(
    test_streaming_anyof_with_markdown_partial,
    STREAMING_ANYOF_BUG,
    r#"```json
{"Description": "Test"#,
    TypeIR::class("Inspiration"),
    {"Description": "Test"}
);

test_partial_deserializer_streaming!(
  test_person_with_check,
  r#"
  class Person {
    known_ages int | null @check(hi, {{ false }})
    name string
  }
  "#,
  r#"{
    known_ages: 10
  "#,
  TypeIR::class("Person"),
  {"known_ages": {
    "value": null,
    "checks": {
      "hi": {
        "name": "hi",
        "expression": "false",
        "status": "failed"
      }
    }
  }, "name": null}
);

// Regression test for nested AnyOf leaking into string output
// This tests the scenario where a user sees "[json AnyOf[{,AnyOf[{,{},],]]i" in output
const NESTED_ANYOF_BUG: &str = r#"
class Response {
  content string
}
"#;

// Test that partial JSON with markdown doesn't leak AnyOf representations
test_partial_deserializer_streaming!(
    test_streaming_nested_anyof_no_leak,
    NESTED_ANYOF_BUG,
    r#"```json
{"content": "[json"#,
    TypeIR::class("Response"),
    {"content": "[json"}
);

// Test with incomplete nested object that might create AnyOf
test_partial_deserializer_streaming!(
    test_streaming_anyof_with_nested_incomplete,
    NESTED_ANYOF_BUG,
    r#"{"content": "test value with {"#,
    TypeIR::class("Response"),
    {"content": "test value with {"}
);
