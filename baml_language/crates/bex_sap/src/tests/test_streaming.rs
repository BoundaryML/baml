//! Streaming (partial parse) behaviour per BEP-075 "Simplified Streaming".
//!
//! There are no stream types and no type attributes: the partial parse uses the declared
//! type, every class field has a default derived from its type, and the only knobs are the
//! declaration attributes `@stream.done`, `@stream.must_exist`, and `@@stream.done`.

use super::*;

// ============================================================================
// Section 1: field defaults while a field is pending (the BEP-075 table)
// ============================================================================

fn table_db() -> TypeRefDb<'static, &'static str> {
    baml_db! {
        enum One { Only }
        class Row {
            text: string,
            list: [int],
            dict: map<string, int>,
            maybe: (int | null),
            literal: "fixed",
            one: One,
        }
    }
}

// Nothing has arrived yet: every field shows its default.
test_partial_deserializer!(
    test_defaults_fill_pending_fields,
    r#"{"#,
    baml_ty!(Row),
    table_db(),
    {"text": "", "list": [], "dict": {}, "maybe": null, "literal": "fixed", "one": "Only"}
);

// Fields fill in as they arrive; the rest keep their defaults.
test_partial_deserializer!(
    test_defaults_replaced_as_fields_arrive,
    r#"{"text": "hi", "list": [1, 2], "dict": {"a": 1}, "maybe": 3, "literal": "fixed", "one": "Only""#,
    baml_ty!(Row),
    table_db(),
    {"text": "hi", "list": [1, 2], "dict": {"a": 1}, "maybe": 3, "literal": "fixed", "one": "Only"}
);

// ============================================================================
// Section 2: a field without a default (`never`) blocks the partial parse
// ============================================================================

fn label_count_db() -> TypeRefDb<'static, &'static str> {
    baml_db! {
        class Bar {
            label: string,
            count: int,
        }
    }
}

// `count` is pending and `int` has no default: no partial parse yet.
test_partial_none_deserializer!(
    test_never_default_pending_blocks_class,
    r#"{"label": "hi""#,
    baml_ty!(Bar),
    label_count_db()
);

// `count` is incomplete and `int` has no partial parse: still nothing.
test_partial_none_deserializer!(
    test_never_default_incomplete_blocks_class,
    r#"{"label": "hi", "count": 1"#,
    baml_ty!(Bar),
    label_count_db()
);

// Once `count` completes the class streams even though the object is still open.
test_partial_deserializer!(
    test_never_default_complete_value_unblocks_class,
    r#"{"count": 12, "label": "h"#,
    baml_ty!(Bar),
    label_count_db(),
    {"label": "h", "count": 12}
);

test_deserializer!(
    test_never_default_complete_object,
    r#"{"label": "hi", "count": 12}"#,
    baml_ty!(Bar),
    label_count_db(),
    {"label": "hi", "count": 12}
);

// ============================================================================
// Section 3: incomplete values with and without a partial parse
// ============================================================================

fn partial_kinds_db() -> TypeRefDb<'static, &'static str> {
    baml_db! {
        class Kinds {
            text: string,
            maybe: (int | null),
            nums: [int],
            words: [string],
        }
    }
}

// A string streams its prefix.
test_partial_deserializer!(
    test_string_streams_its_prefix,
    r#"{"text": "hel"#,
    baml_ty!(Kinds),
    partial_kinds_db(),
    {"text": "hel", "maybe": null, "nums": [], "words": []}
);

// An incomplete number has no partial parse; the nullable field falls back to `null`.
test_partial_deserializer!(
    test_incomplete_number_falls_back_to_null,
    r#"{"text": "hi", "maybe": 1"#,
    baml_ty!(Kinds),
    partial_kinds_db(),
    {"text": "hi", "maybe": null, "nums": [], "words": []}
);

// A list streams its complete items; the incomplete tail number is dropped.
test_partial_deserializer!(
    test_list_drops_incomplete_number,
    r#"{"text": "hi", "nums": [1, 2"#,
    baml_ty!(Kinds),
    partial_kinds_db(),
    {"text": "hi", "maybe": null, "nums": [1], "words": []}
);

// A list of strings keeps the partial tail string.
test_partial_deserializer!(
    test_list_keeps_partial_string,
    r#"{"text": "hi", "nums": [1, 2], "words": ["a", "b"#,
    baml_ty!(Kinds),
    partial_kinds_db(),
    {"text": "hi", "maybe": null, "nums": [1, 2], "words": ["a", "b"]}
);

// Top-level values follow the same column: a bare number has no partial parse...
test_partial_none_deserializer!(
    test_top_level_int_has_no_partial,
    r#"4"#,
    baml_ty!(int),
    baml_db! {}
);

// ...and neither does `int | null` (`null` is not a partial parse of `4`).
test_partial_none_deserializer!(
    test_top_level_nullable_int_has_no_partial,
    r#"4"#,
    baml_ty!((int | null)),
    baml_db! {}
);

test_partial_deserializer!(
    test_top_level_string_streams,
    r#"hello wor"#,
    baml_ty!(string),
    baml_db! {},
    "hello wor"
);

// ============================================================================
// Section 4: `@stream.done` on a field
// ============================================================================

fn stream_done_field_db() -> TypeRefDb<'static, &'static str> {
    baml_db! {
        class Note {
            title: string,
            body: string @stream.done,
        }
    }
}

// The incomplete body is held at its default even though a string could stream.
test_partial_deserializer!(
    test_stream_done_field_holds_default_while_incomplete,
    r#"{"title": "T", "body": "hel"#,
    baml_ty!(Note),
    stream_done_field_db(),
    {"title": "T", "body": ""}
);

// The body is complete even though the object is not.
test_partial_deserializer!(
    test_stream_done_field_shows_complete_value,
    r#"{"title": "T", "body": "hello""#,
    baml_ty!(Note),
    stream_done_field_db(),
    {"title": "T", "body": "hello"}
);

test_partial_deserializer!(
    test_stream_done_field_pending,
    r#"{"title": "T"#,
    baml_ty!(Note),
    stream_done_field_db(),
    {"title": "T", "body": ""}
);

test_deserializer!(
    test_stream_done_field_complete_object,
    r#"{"title": "T", "body": "hello"}"#,
    baml_ty!(Note),
    stream_done_field_db(),
    {"title": "T", "body": "hello"}
);

// `@stream.done` on a list holds the whole list until it closes.
test_partial_deserializer!(
    test_stream_done_list_holds_until_closed,
    r#"{"names": ["a", "b""#,
    baml_ty!(Names),
    baml_db! {
        class Names {
            names: [string] @stream.done,
        }
    },
    {"names": []}
);

// ============================================================================
// Section 5: `@stream.must_exist` on a field
// ============================================================================

fn must_exist_db() -> TypeRefDb<'static, &'static str> {
    baml_db! {
        class Tagged {
            kind: string @stream.must_exist,
            message: string,
        }
    }
}

// The class has no partial parse until `kind` appears.
test_partial_none_deserializer!(
    test_must_exist_pending_blocks_class,
    r#"{"message": "hel"#,
    baml_ty!(Tagged),
    must_exist_db()
);

test_partial_deserializer!(
    test_must_exist_present_streams,
    r#"{"kind": "greeting", "message": "hel"#,
    baml_ty!(Tagged),
    must_exist_db(),
    {"kind": "greeting", "message": "hel"}
);

// `@stream.must_exist` removes the default; an incomplete string still has a partial parse.
test_partial_deserializer!(
    test_must_exist_partial_value_counts_as_present,
    r#"{"kind": "gre"#,
    baml_ty!(Tagged),
    must_exist_db(),
    {"kind": "gre", "message": ""}
);

// A complete object must supply the field.
test_failing_deserializer!(
    test_must_exist_missing_from_complete_object_errors,
    r#"{"message": "hello"}"#,
    baml_ty!(Tagged),
    must_exist_db()
);

// In a list, items without the field drop out.
test_partial_deserializer!(
    test_must_exist_filters_list_items,
    r#"[{"kind": "a", "message": "hi"}, {"message": "wo"#,
    baml_ty!([Tagged]),
    must_exist_db(),
    [{"kind": "a", "message": "hi"}]
);

// ============================================================================
// Section 6: `@@stream.done` on a class
// ============================================================================

fn class_done_db() -> TypeRefDb<'static, &'static str> {
    baml_db! {
        class Done {
            @@stream.done
            name: string,
            value: (int | null),
        }
        class Outer {
            items: [Done],
            current: (Done | null),
        }
    }
}

test_partial_none_deserializer!(
    test_class_done_incomplete_returns_none,
    r#"{"name": "test""#,
    baml_ty!(Done),
    class_done_db()
);

test_deserializer!(
    test_class_done_complete,
    r#"{"name": "test", "value": 42}"#,
    baml_ty!(Done),
    class_done_db(),
    {"name": "test", "value": 42}
);

// Incomplete items drop out of a list.
test_partial_deserializer!(
    test_class_done_filters_list_items,
    r#"[{"name": "a", "value": 1}, {"name": "b""#,
    baml_ty!([Done]),
    class_done_db(),
    [{"name": "a", "value": 1}]
);

// An incomplete nested object holds its field's default.
test_partial_deserializer!(
    test_class_done_nested_holds_default,
    r#"{"items": [{"name": "a", "value": 1}, {"name": "b""#,
    baml_ty!(Outer),
    class_done_db(),
    {"items": [{"name": "a", "value": 1}], "current": null}
);

test_partial_deserializer!(
    test_class_done_nested_nullable_field,
    r#"{"items": [], "current": {"name": "x""#,
    baml_ty!(Outer),
    class_done_db(),
    {"items": [], "current": null}
);

// ============================================================================
// Section 7: class-typed fields default to their fields' defaults
// ============================================================================

fn nested_db() -> TypeRefDb<'static, &'static str> {
    baml_db! {
        class Inner {
            data: string,
            tags: [string],
        }
        class Outer {
            title: string,
            inner: Inner,
        }
        class Counted {
            id: int,
            data: string,
        }
        class Holder {
            title: string,
            counted: Counted,
        }
    }
}

// A pending class field is an object of defaults.
test_partial_deserializer!(
    test_nested_pending_class_is_object_of_defaults,
    r#"{"title": "hel"#,
    baml_ty!(Outer),
    nested_db(),
    {"title": "hel", "inner": {"data": "", "tags": []}}
);

test_partial_deserializer!(
    test_nested_incomplete_class_streams,
    r#"{"title": "hello", "inner": {"data": "tes"#,
    baml_ty!(Outer),
    nested_db(),
    {"title": "hello", "inner": {"data": "tes", "tags": []}}
);

// A nested class with a `never` field has no default, so its holder has none either.
test_partial_none_deserializer!(
    test_nested_never_default_blocks_holder,
    r#"{"title": "hel"#,
    baml_ty!(Holder),
    nested_db()
);

test_partial_deserializer!(
    test_nested_never_default_present,
    r#"{"title": "hi", "counted": {"id": 1, "data": "x"#,
    baml_ty!(Holder),
    nested_db(),
    {"title": "hi", "counted": {"id": 1, "data": "x"}}
);

// The nested object is incomplete and its `id` is pending: nothing to show yet.
test_partial_none_deserializer!(
    test_nested_never_default_pending_inside_blocks_holder,
    r#"{"title": "hi", "counted": {"data": "x"#,
    baml_ty!(Holder),
    nested_db()
);

// ============================================================================
// Section 8: unions
// ============================================================================

fn union_db() -> TypeRefDb<'static, &'static str> {
    baml_db! {
        class ToolCall {
            name: string @stream.must_exist,
            parameters: (string | null),
        }
        class Message {
            role: string @stream.must_exist,
            content: (string | null),
        }
    }
}

test_partial_deserializer!(
    test_union_partial_tool_call,
    r#"{"name": "get_weather", "parameters": "{"#,
    baml_ty!((ToolCall | Message)),
    union_db(),
    {"name": "get_weather", "parameters": "{"}
);

test_partial_deserializer!(
    test_union_partial_message,
    r#"{"role": "assistant", "content": "hel"#,
    baml_ty!((ToolCall | Message)),
    union_db(),
    {"role": "assistant", "content": "hel"}
);

// Neither member has its `@stream.must_exist` field yet.
test_partial_none_deserializer!(
    test_union_no_member_has_partial,
    r#"{"parameters": "{"#,
    baml_ty!((ToolCall | Message)),
    union_db()
);

test_deserializer!(
    test_union_complete,
    r#"{"name": "get_weather", "parameters": "{}"}"#,
    baml_ty!((ToolCall | Message)),
    union_db(),
    {"name": "get_weather", "parameters": "{}"}
);

// A union of types without partial parses has none.
test_partial_none_deserializer!(
    test_union_of_atoms_has_no_partial,
    r#"1"#,
    baml_ty!((int | bool)),
    baml_db! {}
);

// A union is partial when its matching member is.
test_partial_deserializer!(
    test_union_streams_through_string_member,
    r#""hel"#,
    baml_ty!((int | string)),
    baml_db! {},
    "hel"
);

// ============================================================================
// Section 9: lists drop items without a partial parse
// ============================================================================

fn scored_db() -> TypeRefDb<'static, &'static str> {
    baml_db! {
        class Scored {
            name: string,
            score: int,
        }
    }
}

test_partial_deserializer!(
    test_list_drops_item_without_partial,
    r#"[{"name": "a", "score": 1}, {"name": "b""#,
    baml_ty!([Scored]),
    scored_db(),
    [{"name": "a", "score": 1}]
);

test_partial_deserializer!(
    test_list_all_items_without_partial_is_empty,
    r#"[{"name": "a""#,
    baml_ty!([Scored]),
    scored_db(),
    []
);

test_deserializer!(
    test_list_complete,
    r#"[{"name": "a", "score": 1}, {"name": "b", "score": 2}]"#,
    baml_ty!([Scored]),
    scored_db(),
    [{"name": "a", "score": 1}, {"name": "b", "score": 2}]
);

test_partial_deserializer!(
    test_nested_list_streams_inner_lists,
    r#"[[1, 2], [3"#,
    baml_ty!([[int]]),
    baml_db! {},
    [[1, 2], []]
);

// ============================================================================
// Section 10: maps drop entries without a partial parse
// ============================================================================

test_partial_deserializer!(
    test_map_drops_entry_without_partial,
    r#"{"a": 1, "b": 2"#,
    baml_ty!(map<string, int>),
    baml_db! {},
    {"a": 1}
);

test_partial_deserializer!(
    test_map_keeps_partial_string_entry,
    r#"{"a": "x", "b": "y"#,
    baml_ty!(map<string, string>),
    baml_db! {},
    {"a": "x", "b": "y"}
);

// ============================================================================
// Section 11: `StreamState`
//
// `StreamState` wraps a value with {"value": ..., "state": "Pending"/"Incomplete"/"Complete"}.
// ============================================================================

fn stream_state_db() -> TypeRefDb<'static, &'static str> {
    baml_db! {
        class Foo {
            nums: [int],
            bar: (int | null),
        }
        class MultiStream {
            name: StreamState<string>,
            note: StreamState<string>,
            label: (string | null),
        }
    }
}

test_partial_deserializer!(
    test_stream_state_incomplete_class,
    r#"{"nums": [1, 2"#,
    baml_ty!(StreamState<Foo>),
    stream_state_db(),
    {"value": {"nums": [1], "bar": null}, "state": "Incomplete"}
);

test_deserializer!(
    test_stream_state_complete_class,
    r#"{"nums": [1, 2], "bar": 3}"#,
    baml_ty!(StreamState<Foo>),
    stream_state_db(),
    {"value": {"nums": [1, 2], "bar": 3}, "state": "Complete"}
);

test_partial_deserializer!(
    test_stream_state_string_incomplete,
    r#""hel"#,
    baml_ty!(StreamState<string>),
    baml_db! {},
    {"value": "hel", "state": "Incomplete"}
);

test_deserializer!(
    test_stream_state_string_complete,
    r#""hello""#,
    baml_ty!(StreamState<string>),
    baml_db! {},
    {"value": "hello", "state": "Complete"}
);

test_deserializer!(
    test_stream_state_int_complete,
    r#"42"#,
    baml_ty!(StreamState<int>),
    baml_db! {},
    {"value": 42, "state": "Complete"}
);

// The inner type has no partial parse, so neither does the wrapper.
test_partial_none_deserializer!(
    test_stream_state_int_incomplete,
    r#"4"#,
    baml_ty!(StreamState<int>),
    baml_db! {}
);

// A pending `StreamState` field is `Pending` around the inner default.
test_partial_deserializer!(
    test_stream_state_fields_partial,
    r#"{"name": "hel"#,
    baml_ty!(MultiStream),
    stream_state_db(),
    {
        "name": {"value": "hel", "state": "Incomplete"},
        "note": {"value": "", "state": "Pending"},
        "label": null
    }
);

test_deserializer!(
    test_stream_state_fields_complete,
    r#"{"name": "hello", "note": "n", "label": "done"}"#,
    baml_ty!(MultiStream),
    stream_state_db(),
    {
        "name": {"value": "hello", "state": "Complete"},
        "note": {"value": "n", "state": "Complete"},
        "label": "done"
    }
);

// A `StreamState` around a type without a default has none either.
test_partial_none_deserializer!(
    test_stream_state_never_inner_blocks_class,
    r#"{"label": "x"#,
    baml_ty!(Counter),
    baml_db! {
        class Counter {
            count: StreamState<int>,
            label: string,
        }
    }
);

// ============================================================================
// Section 12: AnyOf regression tests
//
// Ensure that partial JSON with markdown doesn't leak internal AnyOf
// representations into string output.
// ============================================================================

fn anyof_db() -> TypeRefDb<'static, &'static str> {
    baml_db! {
        class Inspiration {
            Description: (string | null),
        }
        class Response {
            content: (string | null),
        }
    }
}

test_partial_deserializer!(
    test_anyof_string_field,
    r#"{"Description": "A beautiful sunset over the ocean"#,
    baml_ty!(Inspiration),
    anyof_db(),
    {"Description": "A beautiful sunset over the ocean"}
);

test_partial_deserializer!(
    test_anyof_with_markdown_partial,
    r#"```json
{"Description": "Test"#,
    baml_ty!(Inspiration),
    anyof_db(),
    {"Description": "Test"}
);

test_partial_deserializer!(
    test_nested_anyof_no_leak,
    r#"```json
{"content": "[json"#,
    baml_ty!(Response),
    anyof_db(),
    {"content": "[json"}
);

test_partial_deserializer!(
    test_anyof_with_nested_incomplete,
    r#"{"content": "test value with {"#,
    baml_ty!(Response),
    anyof_db(),
    {"content": "test value with {"}
);

// ============================================================================
// Section 13: complete objects and missing fields
// ============================================================================

fn missing_db() -> TypeRefDb<'static, &'static str> {
    baml_db! {
        class Containers {
            items: [string],
            data: map<string, int>,
            opt: (string | null),
        }
        class StrictItem {
            id: int,
            name: string,
        }
    }
}

// Arrays, maps, and nullable fields may be omitted from a complete object.
test_deserializer!(
    test_complete_missing_containers_default,
    r#"{}"#,
    baml_ty!(Containers),
    missing_db(),
    {"items": [], "data": {}, "opt": null}
);

// Anything else is required.
test_failing_deserializer!(
    test_complete_missing_required_field_errors,
    r#"{"id": 1}"#,
    baml_ty!(StrictItem),
    missing_db()
);

test_deserializer!(
    test_complete_all_fields_present,
    r#"{"id": 1, "name": "test"}"#,
    baml_ty!(StrictItem),
    missing_db(),
    {"id": 1, "name": "test"}
);

// While streaming, the same missing field takes its default instead.
test_partial_deserializer!(
    test_partial_uses_default_where_complete_would_error,
    r#"{"id": 1, "#,
    baml_ty!(StrictItem),
    missing_db(),
    {"id": 1, "name": ""}
);

// ============================================================================
// Section 14: edge cases - empty objects, deeply nested, etc.
// ============================================================================

fn edge_case_db() -> TypeRefDb<'static, &'static str> {
    baml_db! {
        class Leaf {
            value: (string | null),
        }
        class Branch {
            leaf: (Leaf | null),
            children: [Branch],
        }
    }
}

// Empty incomplete object
test_partial_deserializer!(
    test_edge_empty_incomplete_object,
    r#"{"#,
    baml_ty!(Branch),
    edge_case_db(),
    {"leaf": null, "children": []}
);

// Deeply nested partial
test_partial_deserializer!(
    test_edge_deeply_nested_partial,
    r#"{"leaf": {"value": "root"}, "children": [{"leaf": {"value": "child"}, "children": [{"leaf": {"value": "grandch"#,
    baml_ty!(Branch),
    edge_case_db(),
    {
        "leaf": {"value": "root"},
        "children": [
            {
                "leaf": {"value": "child"},
                "children": [
                    {
                        "leaf": {"value": "grandch"},
                        "children": []
                    }
                ]
            }
        ]
    }
);

// Complete deeply nested
test_deserializer!(
    test_edge_deeply_nested_complete,
    r#"{"leaf": {"value": "root"}, "children": [{"leaf": {"value": "child"}, "children": []}]}"#,
    baml_ty!(Branch),
    edge_case_db(),
    {
        "leaf": {"value": "root"},
        "children": [
            {
                "leaf": {"value": "child"},
                "children": []
            }
        ]
    }
);

// ============================================================================
// Section 15: `json`-typed stream
//
// `baml.json.json` is modeled via the same `JsonValue` recursive alias used in
// `test_aliases.rs`. With no type attributes there is nothing to hold a json
// value back: it streams structurally, like any other type.
// ============================================================================

fn json_value_db_for_streaming() -> TypeRefDb<'static, &'static str> {
    baml_db! {
        type JsonValueArr = [JsonValue];
        type JsonValueMap = (map<string, JsonValue>);
        type JsonValue = (int | float | bool | string | null | JsonValueArr | JsonValueMap);
    }
}

test_partial_deserializer!(
    test_json_stream_incomplete_object,
    r#"{"key": "val"#,
    baml_ty!((JsonValue | null)),
    json_value_db_for_streaming(),
    {"key": "val"}
);

test_partial_deserializer!(
    test_json_stream_incomplete_array,
    r#"[1, 2, 3"#,
    baml_ty!((JsonValue | null)),
    json_value_db_for_streaming(),
    [1, 2]
);

test_partial_deserializer!(
    test_json_stream_incomplete_string,
    r#""hello"#,
    baml_ty!((JsonValue | null)),
    json_value_db_for_streaming(),
    "hello"
);

test_deserializer!(
    test_json_stream_complete_object,
    r#"{"key": "value", "num": 42}"#,
    baml_ty!((JsonValue | null)),
    json_value_db_for_streaming(),
    {"key": "value", "num": 42}
);

test_deserializer!(
    test_json_stream_complete_array,
    r#"[1, 2, 3]"#,
    baml_ty!((JsonValue | null)),
    json_value_db_for_streaming(),
    [1, 2, 3]
);

test_deserializer!(
    test_json_stream_complete_null_value,
    r#"null"#,
    baml_ty!(JsonValue),
    json_value_db_for_streaming(),
    null
);

test_deserializer!(
    test_json_stream_complete_number,
    r#"42"#,
    baml_ty!((JsonValue | null)),
    json_value_db_for_streaming(),
    42
);

test_deserializer!(
    test_json_stream_complete_nested_object,
    r#"{"a": {"b": [1, 2, null]}}"#,
    baml_ty!((JsonValue | null)),
    json_value_db_for_streaming(),
    {"a": {"b": [1, 2, null]}}
);
