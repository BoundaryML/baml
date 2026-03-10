use super::*;

// ============================================================================
// Helpers
// ============================================================================

/// Simple class with configurable in_progress and class_in_progress_field_missing.
fn simple_db() -> TypeRefDb<'static, &'static str> {
    baml_db! {
        class Foo {
            nums: [int] @class_in_progress_field_missing([]),
        }
    }
}

// ============================================================================
// Section 1: @in_progress attribute on types
//
// - None (default) => returns partial value
// - never          => returns None
// - <value>        => returns that value
// - invalid type   => error
// ============================================================================

// --- in_progress = None (default): partial value returned ---

test_partial_deserializer!(
    test_in_progress_default_partial,
    r#"{"nums": [1, 2"#,
    baml_tyannotated!(Foo),
    simple_db(),
    {"nums": [1, 2]}
);

// --- in_progress = never: returns None ---

test_partial_none_deserializer!(
    test_in_progress_never_returns_none,
    r#"{"nums": [1, 2"#,
    baml_tyannotated!(Foo @in_progress(never)),
    simple_db()
);

// --- in_progress = <same type value>: returns that value ---

test_partial_deserializer!(
    test_in_progress_value_string,
    r#"{"name": "hel"#,
    baml_tyannotated!(string @in_progress("Loading...")),
    baml_db! {},
    "Loading..."
);

// --- in_progress = never on a completed value passes through ---

test_deserializer!(
    test_in_progress_never_on_complete_value,
    r#"{"nums": [1, 2]}"#,
    baml_tyannotated!(Foo @in_progress(never)),
    simple_db(),
    {"nums": [1, 2]}
);

// ============================================================================
// Section 2: @class_in_progress_field_missing attribute
//
// Controls what happens when a field is missing from an incomplete class object.
// - never => class returns None (field required before class is visible)
// - null  => field gets null
// - <value> => field gets that value
// ============================================================================

// --- class_in_progress_field_missing = null: missing fields get null ---

fn class_missing_null_db() -> TypeRefDb<'static, &'static str> {
    baml_db! {
        class Person {
            name: (string | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
            age: (int | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
        }
    }
}

test_partial_deserializer!(
    test_class_missing_null_partial_one_field,
    r#"{"name": "Ali"#,
    baml_tyannotated!(Person),
    class_missing_null_db(),
    {"name": "Ali", "age": null}
);

test_partial_deserializer!(
    test_class_missing_null_no_fields,
    r#"{"#,
    baml_tyannotated!(Person),
    class_missing_null_db(),
    {"name": null, "age": null}
);

// --- class_in_progress_field_missing = never: class excluded until field present ---

fn class_missing_never_db() -> TypeRefDb<'static, &'static str> {
    baml_db! {
        class Item {
            id: int @class_in_progress_field_missing(never) @class_completed_field_missing(never),
            label: (string | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
        }
    }
}

// When id is missing (incomplete), class_in_progress_field_missing=never means class is excluded => None
test_partial_none_deserializer!(
    test_class_missing_never_excludes_class,
    r#"{"label": "hel"#,
    baml_tyannotated!(Item @in_progress(never)),
    class_missing_never_db()
);

// When id IS present, class succeeds even if incomplete
test_partial_deserializer!(
    test_class_missing_never_present_field_succeeds,
    r#"{"id": 42"#,
    baml_tyannotated!(Item),
    class_missing_never_db(),
    {"id": 42, "label": null}
);

// --- class_in_progress_field_missing = <value>: field gets that value ---

fn class_missing_default_value_db() -> TypeRefDb<'static, &'static str> {
    baml_db! {
        class Config {
            name: string @class_in_progress_field_missing("pending") @class_completed_field_missing("unknown"),
            count: int @class_in_progress_field_missing(0) @class_completed_field_missing(0),
        }
    }
}

test_partial_deserializer!(
    test_class_missing_default_value_partial,
    r#"{"name": "tes"#,
    baml_tyannotated!(Config),
    class_missing_default_value_db(),
    {"name": "tes", "count": 0}
);

test_partial_deserializer!(
    test_class_missing_default_value_empty,
    r#"{"#,
    baml_tyannotated!(Config),
    class_missing_default_value_db(),
    {"name": "pending", "count": 0}
);

// Complete object uses class_completed_field_missing
test_deserializer!(
    test_class_completed_missing_default_value,
    r#"{"name": "test"}"#,
    baml_tyannotated!(Config),
    class_missing_default_value_db(),
    {"name": "test", "count": 0}
);

// ============================================================================
// Section 3: @class_completed_field_missing attribute
//
// Controls what happens when a field is missing from a complete class object.
// - never => error (field required in complete object)
// - null  => field gets null
// - <value> => field gets that value
// ============================================================================

fn class_completed_never_db() -> TypeRefDb<'static, &'static str> {
    baml_db! {
        class StrictItem {
            id: int @class_in_progress_field_missing(null) @class_completed_field_missing(never),
            name: string @class_in_progress_field_missing(null) @class_completed_field_missing(never),
        }
    }
}

// Complete object missing a field with class_completed_field_missing=never => error
test_failing_deserializer!(
    test_class_completed_never_missing_field_errors,
    r#"{"id": 1}"#,
    baml_tyannotated!(StrictItem),
    class_completed_never_db()
);

// Complete object with all fields present => success
test_deserializer!(
    test_class_completed_never_all_fields_success,
    r#"{"id": 1, "name": "test"}"#,
    baml_tyannotated!(StrictItem),
    class_completed_never_db(),
    {"id": 1, "name": "test"}
);

// Incomplete object can still use class_in_progress_field_missing=null
test_partial_deserializer!(
    test_class_completed_never_but_partial_uses_in_progress,
    r#"{"id": 1"#,
    baml_tyannotated!(StrictItem),
    class_completed_never_db(),
    {"id": 1, "name": null}
);

// ============================================================================
// Section 4: Combinations of @in_progress and @class_in_progress_field_missing
// ============================================================================

// --- in_progress(never) + class_in_progress_field_missing(null) ---
// The in_progress(never) on the type takes precedence: incomplete object => None

fn combo_never_null_db() -> TypeRefDb<'static, &'static str> {
    baml_db! {
        class Widget {
            name: (string | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
            count: (int | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
        }
    }
}

test_partial_none_deserializer!(
    test_combo_in_progress_never_class_missing_null,
    r#"{"name": "wid"#,
    baml_tyannotated!(Widget @in_progress(never)),
    combo_never_null_db()
);

// But when complete, it succeeds
test_deserializer!(
    test_combo_in_progress_never_class_missing_null_complete,
    r#"{"name": "widget", "count": 5}"#,
    baml_tyannotated!(Widget @in_progress(never)),
    combo_never_null_db(),
    {"name": "widget", "count": 5}
);

// --- in_progress(never) on inner field type + class_in_progress_field_missing(never) ---
// Both are never: incomplete class excluded

fn combo_both_never_db() -> TypeRefDb<'static, &'static str> {
    baml_db! {
        class Gate {
            key: string @in_progress(never) @class_in_progress_field_missing(never) @class_completed_field_missing(never),
            value: (string | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
        }
    }
}

test_partial_none_deserializer!(
    test_combo_both_never_missing_key,
    r#"{"value": "hel"#,
    baml_tyannotated!(Gate @in_progress(never)),
    combo_both_never_db()
);

// key is present and complete => class works
test_partial_deserializer!(
    test_combo_both_never_key_present,
    r#"{"key": "abc", "value": "hel"#,
    baml_tyannotated!(Gate),
    combo_both_never_db(),
    {"key": "abc", "value": "hel"}
);

// --- class_in_progress_field_missing(never) on required field ---
// Class not returned until required field appears, then partial class returned

fn combo_required_field_db() -> TypeRefDb<'static, &'static str> {
    baml_db! {
        class Order {
            order_id: int @class_in_progress_field_missing(never),
            items: [string] @class_in_progress_field_missing([]),
            note: (string | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
        }
    }
}

// Missing order_id => class excluded
test_partial_none_deserializer!(
    test_required_field_missing_excludes_class,
    r#"{"items": ["apple""#,
    baml_tyannotated!(Order @in_progress(never)),
    combo_required_field_db()
);

// order_id present => class returned with partial data
test_partial_deserializer!(
    test_required_field_present_partial,
    r#"{"order_id": 123, "items": ["apple""#,
    baml_tyannotated!(Order),
    combo_required_field_db(),
    {"order_id": 123, "items": ["apple"], "note": null}
);

// ============================================================================
// Section 5: StreamState (the @stream.with_state wrapper)
//
// StreamState wraps a value with {"value": ..., "state": "Complete"/"Incomplete"}
// ============================================================================

fn stream_state_db() -> TypeRefDb<'static, &'static str> {
    baml_db! {
        class Foo {
            nums: [int] @class_in_progress_field_missing([]),
            bar: (int | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
        }
    }
}

// Incomplete StreamState<int[]>
test_partial_deserializer!(
    test_stream_state_incomplete_list,
    r#"{"nums": [1, 2"#,
    baml_tyannotated!(StreamState<Foo>),
    stream_state_db(),
    {"value": {"nums": [1, 2], "bar": null}, "state": "Incomplete"}
);

// Complete StreamState
test_deserializer!(
    test_stream_state_complete,
    r#"{"nums": [1, 2], "bar": 3}"#,
    baml_tyannotated!(StreamState<Foo>),
    stream_state_db(),
    {"value": {"nums": [1, 2], "bar": 3}, "state": "Complete"}
);

// StreamState with nested class fields having their own StreamState
fn stream_state_nested_db() -> TypeRefDb<'static, &'static str> {
    baml_db! {
        class Bar {
            message: string @class_in_progress_field_missing(null),
            count: (int | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
        }
    }
}

test_partial_deserializer!(
    test_stream_state_field_incomplete,
    r#"{"message": "hel"#,
    baml_tyannotated!(StreamState<Bar>),
    stream_state_nested_db(),
    {"value": {"message": "hel", "count": null}, "state": "Incomplete"}
);

test_deserializer!(
    test_stream_state_field_complete,
    r#"{"message": "hello", "count": 5}"#,
    baml_tyannotated!(StreamState<Bar>),
    stream_state_nested_db(),
    {"value": {"message": "hello", "count": 5}, "state": "Complete"}
);

// StreamState on a primitive
test_partial_deserializer!(
    test_stream_state_string_incomplete,
    r#""hel"#,
    baml_tyannotated!(StreamState<string>),
    baml_db!{},
    {"value": "hel", "state": "Incomplete"}
);

test_deserializer!(
    test_stream_state_string_complete,
    r#""hello""#,
    baml_tyannotated!(StreamState<string>),
    baml_db!{},
    {"value": "hello", "state": "Complete"}
);

// StreamState on an int
test_deserializer!(
    test_stream_state_int_complete,
    r#"42"#,
    baml_tyannotated!(StreamState<int>),
    baml_db!{},
    {"value": 42, "state": "Complete"}
);

// ============================================================================
// Section 6: StreamState combined with @in_progress
// ============================================================================

// StreamState with in_progress(never) on inner type: incomplete => None propagates through StreamState
test_partial_none_deserializer!(
    test_stream_state_in_progress_never,
    r#"{"nums": [1, 2"#,
    baml_tyannotated!(StreamState<Foo @in_progress(never)>),
    stream_state_db()
);

// StreamState with in_progress value
test_partial_deserializer!(
    test_stream_state_in_progress_string_value,
    r#""hel"#,
    baml_tyannotated!(StreamState<string @in_progress("...")>),
    baml_db!{},
    {"value": "...", "state": "Incomplete"}
);

test_deserializer!(
    test_stream_state_in_progress_string_complete,
    r#""hello""#,
    baml_tyannotated!(StreamState<string @in_progress("...")>),
    baml_db!{},
    {"value": "hello", "state": "Complete"}
);

// ============================================================================
// Section 7: Nested classes with mixed streaming attributes
// ============================================================================

fn nested_streaming_db() -> TypeRefDb<'static, &'static str> {
    baml_db! {
        class Inner {
            id: int @class_in_progress_field_missing(never),
            data: (string | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
        }
        class Outer {
            title: (string | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
            inner: (Inner | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
            items: [Inner] @class_in_progress_field_missing([]),
        }
    }
}

// Outer is partial, inner is not yet started => inner is null
test_partial_deserializer!(
    test_nested_outer_partial_inner_missing,
    r#"{"title": "hel"#,
    baml_tyannotated!(Outer),
    nested_streaming_db(),
    {"title": "hel", "inner": null, "items": []}
);

// Inner started but id missing => inner field excluded (never), falls back to null
test_partial_deserializer!(
    test_nested_inner_missing_required_field,
    r#"{"title": "hello", "inner": {"data": "tes"#,
    baml_tyannotated!(Outer),
    nested_streaming_db(),
    {"title": "hello", "inner": null, "items": []}
);

// Inner has id => inner is returned
test_partial_deserializer!(
    test_nested_inner_has_required_field,
    r#"{"title": "hello", "inner": {"id": 1, "data": "tes"#,
    baml_tyannotated!(Outer),
    nested_streaming_db(),
    {"title": "hello", "inner": {"id": 1, "data": "tes"}, "items": []}
);

// Items list with partial inner objects
test_partial_deserializer!(
    test_nested_items_list_partial,
    r#"{"title": "hello", "items": [{"id": 1, "data": "done"}, {"id": 2"#,
    baml_tyannotated!(Outer),
    nested_streaming_db(),
    {"title": "hello", "inner": null, "items": [{"id": 1, "data": "done"}, {"id": 2, "data": null}]}
);

// Items list where an incomplete item is missing the required id => filtered
test_partial_deserializer!(
    test_nested_items_list_incomplete_item_no_id,
    r#"{"title": "hello", "items": [{"id": 1, "data": "done"}, {"data": "tes"#,
    baml_tyannotated!(Outer),
    nested_streaming_db(),
    {"title": "hello", "inner": null, "items": [{"id": 1, "data": "done"}]}
);

// ============================================================================
// Section 8: Union types with streaming
// ============================================================================

fn union_streaming_db() -> TypeRefDb<'static, &'static str> {
    baml_db! {
        class ToolCall {
            name: string @class_in_progress_field_missing(never),
            parameters: (string | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
        }
        class Message {
            role: string @class_in_progress_field_missing(never),
            content: (string | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
        }
    }
}

// Union partial: one variant matches
test_partial_deserializer!(
    test_union_partial_tool_call,
    r#"{"name": "get_weather", "parameters": "{"#,
    baml_tyannotated!((ToolCall | Message)),
    union_streaming_db(),
    {"name": "get_weather", "parameters": "{"}
);

test_partial_deserializer!(
    test_union_partial_message,
    r#"{"role": "assistant", "content": "hel"#,
    baml_tyannotated!((ToolCall | Message)),
    union_streaming_db(),
    {"role": "assistant", "content": "hel"}
);

// Union with in_progress(never): incomplete => None
test_partial_none_deserializer!(
    test_union_in_progress_never,
    r#"{"name": "get_weather"#,
    baml_tyannotated!((ToolCall | Message) @in_progress(never)),
    union_streaming_db()
);

// Union complete works normally
test_deserializer!(
    test_union_complete,
    r#"{"name": "get_weather", "parameters": "{}"}"#,
    baml_tyannotated!((ToolCall | Message)),
    union_streaming_db(),
    {"name": "get_weather", "parameters": "{}"}
);

// ============================================================================
// Section 9: List of unions with in_progress(never) filtering
//
// When items in a list have @in_progress(never), incomplete items should be
// filtered out during streaming.
// ============================================================================

fn list_union_done_db() -> TypeRefDb<'static, &'static str> {
    baml_db! {
        class ToolCall {
            name: string,
            parameters: string,
        }
        class ExampleMessage {
            role: string,
            content: string,
        }
    }
}

// Incomplete item filtered out
test_partial_deserializer!(
    test_list_union_done_incomplete_filtered,
    r#"[{"name": "get_weather", "parameters": "{}"}, {"name": "add_reminder"#,
    baml_tyannotated!([(ToolCall | ExampleMessage) @in_progress(never)]),
    list_union_done_db(),
    [{"name": "get_weather", "parameters": "{}"}]
);

// All incomplete => empty list
test_partial_deserializer!(
    test_list_union_done_all_incomplete,
    r#"[{"name": "get_weather"#,
    baml_tyannotated!([(ToolCall | ExampleMessage) @in_progress(never)]),
    list_union_done_db(),
    []
);

// All complete => all items
test_deserializer!(
    test_list_union_done_all_complete,
    r#"[{"name": "get_weather", "parameters": "{}"}, {"role": "assistant", "content": "hello"}]"#,
    baml_tyannotated!([(ToolCall | ExampleMessage) @in_progress(never)]),
    list_union_done_db(),
    [{"name": "get_weather", "parameters": "{}"}, {"role": "assistant", "content": "hello"}]
);

// ============================================================================
// Section 10: in_progress(never) on class-level (@@stream.done equivalent)
//
// When a class has @in_progress(never), incomplete instances are excluded.
// In a list context, incomplete items are filtered.
// ============================================================================

fn class_done_db() -> TypeRefDb<'static, &'static str> {
    baml_db! {
        class DoneItem {
            name: string,
            value: int,
        }
    }
}

// Single incomplete class with in_progress(never) => None
test_partial_none_deserializer!(
    test_class_done_incomplete_returns_none,
    r#"{"name": "test""#,
    baml_tyannotated!(DoneItem @in_progress(never)),
    class_done_db()
);

// Complete class works
test_deserializer!(
    test_class_done_complete,
    r#"{"name": "test", "value": 42}"#,
    baml_tyannotated!(DoneItem @in_progress(never)),
    class_done_db(),
    {"name": "test", "value": 42}
);

// List of done-items: incomplete items filtered
test_partial_deserializer!(
    test_list_class_done_incomplete_filtered,
    r#"[{"name": "a", "value": 1}, {"name": "b""#,
    baml_tyannotated!([DoneItem @in_progress(never)]),
    class_done_db(),
    [{"name": "a", "value": 1}]
);

// ============================================================================
// Section 11: Nested done classes
//
// Inner class with in_progress(never), inside an outer list.
// Incomplete inner objects filtered.
// ============================================================================

fn nested_done_db() -> TypeRefDb<'static, &'static str> {
    baml_db! {
        class DoneFoo {
            nums: [int],
        }
        class DoneBar {
            foos: [DoneFoo @in_progress(never)] @class_in_progress_field_missing([]),
        }
    }
}

test_partial_deserializer!(
    test_nested_done_incomplete_inner_filtered,
    r#"{"foos": [{"nums": [1, 2]}, {"nums": [3, 4"#,
    baml_tyannotated!(DoneBar),
    nested_done_db(),
    {"foos": [{"nums": [1, 2]}]}
);

test_deserializer!(
    test_nested_done_all_complete,
    r#"{"foos": [{"nums": [1, 2]}, {"nums": [3, 4]}]}"#,
    baml_tyannotated!(DoneBar),
    nested_done_db(),
    {"foos": [{"nums": [1, 2]}, {"nums": [3, 4]}]}
);

// ============================================================================
// Section 12: @stream.not_null equivalent (class_in_progress_field_missing(never))
//
// Fields with class_in_progress_field_missing(never) cause the class to not
// be returned until that field has a value.
// ============================================================================

fn not_null_field_db() -> TypeRefDb<'static, &'static str> {
    baml_db! {
        class TypedMessage {
            r#type: string @class_in_progress_field_missing(never),
            message: (string | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
        }
    }
}

// type field missing => class excluded
test_partial_none_deserializer!(
    test_not_null_field_missing_excludes,
    r#"{"message": "hel"#,
    baml_tyannotated!(TypedMessage @in_progress(never)),
    not_null_field_db()
);

// type field present => class returned
test_partial_deserializer!(
    test_not_null_field_present,
    r#"{"type": "greeting", "message": "hel"#,
    baml_tyannotated!(TypedMessage),
    not_null_field_db(),
    {"type": "greeting", "message": "hel"}
);

// In a list: items without type field are filtered
test_partial_deserializer!(
    test_not_null_field_list_filtering,
    r#"[{"type": "a", "message": "hi"}, {"message": "wo"#,
    baml_tyannotated!([TypedMessage @in_progress(never)]),
    not_null_field_db(),
    [{"type": "a", "message": "hi"}]
);

// ============================================================================
// Section 13: Literal type fields with streaming
//
// Literal types used as discriminators in union streaming.
// ============================================================================

fn literal_union_db() -> TypeRefDb<'static, &'static str> {
    baml_db! {
        class MessageToUser {
            r#type: "message_to_user" @class_in_progress_field_missing(never),
            message: (string | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
        }
        class AddItem {
            r#type: "add_item" @class_in_progress_field_missing(never),
            title: (string | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
        }
        class GetLastItemId {
            r#type: "get_last_item_id" @class_in_progress_field_missing(never),
        }
    }
}

test_partial_deserializer!(
    test_literal_union_message,
    r#"{"type": "message_to_user", "message": "Hello us"#,
    baml_tyannotated!((MessageToUser | AddItem | GetLastItemId)),
    literal_union_db(),
    {"type": "message_to_user", "message": "Hello us"}
);

test_partial_deserializer!(
    test_literal_union_add_item,
    r#"{"type": "add_item", "title": "Buy gro"#,
    baml_tyannotated!((MessageToUser | AddItem | GetLastItemId)),
    literal_union_db(),
    {"type": "add_item", "title": "Buy gro"}
);

// GetLastItemId: type field present, no other fields needed
test_partial_deserializer!(
    test_literal_union_get_last_item_id,
    r#"{"type": "get_last_item_id"}"#,
    baml_tyannotated!((MessageToUser | AddItem | GetLastItemId)),
    literal_union_db(),
    {"type": "get_last_item_id"}
);

// Incomplete with done behavior: class_in_progress_field_missing(never) + in_progress(never) on union
test_partial_none_deserializer!(
    test_literal_union_incomplete_done,
    r#"{"type": "add_item", "title": "Buy"#,
    baml_tyannotated!((MessageToUser | AddItem | GetLastItemId) @in_progress(never)),
    literal_union_db()
);

// ============================================================================
// Section 14: Complex nested scenario with multiple streaming attributes
// ============================================================================

fn complex_streaming_db() -> TypeRefDb<'static, &'static str> {
    baml_db! {
        class SmallThing {
            i_value: int @class_in_progress_field_missing(never),
            label: (string | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
        }
        class Container {
            number: (int | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
            text: (string | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
            things: [SmallThing @in_progress(never)] @class_in_progress_field_missing([]),
            required_thing: (SmallThing | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
        }
    }
}

// Partial container: things list filters incomplete items
test_partial_deserializer!(
    test_complex_partial_things_filtered,
    r#"{"number": 42, "things": [{"i_value": 1, "label": "a"}, {"i_value": 2"#,
    baml_tyannotated!(Container),
    complex_streaming_db(),
    {"number": 42, "text": null, "things": [{"i_value": 1, "label": "a"}], "required_thing": null}
);

// Partial container: required_thing inner class missing required field => null
test_partial_deserializer!(
    test_complex_required_thing_missing_field,
    r#"{"number": 42, "required_thing": {"label": "test""#,
    baml_tyannotated!(Container),
    complex_streaming_db(),
    {"number": 42, "text": null, "things": [], "required_thing": null}
);

// Partial container: required_thing has required field
test_partial_deserializer!(
    test_complex_required_thing_has_field,
    r#"{"number": 42, "required_thing": {"i_value": 99, "label": "test""#,
    baml_tyannotated!(Container),
    complex_streaming_db(),
    {"number": 42, "text": null, "things": [], "required_thing": {"i_value": 99, "label": "test"}}
);

// Complete container
test_deserializer!(
    test_complex_complete,
    r#"{"number": 42, "text": "hello", "things": [{"i_value": 1, "label": "a"}], "required_thing": {"i_value": 99, "label": "b"}}"#,
    baml_tyannotated!(Container),
    complex_streaming_db(),
    {"number": 42, "text": "hello", "things": [{"i_value": 1, "label": "a"}], "required_thing": {"i_value": 99, "label": "b"}}
);

// ============================================================================
// Section 15: AnyOf regression tests
//
// Ensure that partial JSON with markdown doesn't leak internal AnyOf
// representations into string output.
// ============================================================================

fn anyof_db() -> TypeRefDb<'static, &'static str> {
    baml_db! {
        class Inspiration {
            Description: (string | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
        }
    }
}

test_partial_deserializer!(
    test_anyof_string_field,
    r#"{"Description": "A beautiful sunset over the ocean"#,
    baml_tyannotated!(Inspiration),
    anyof_db(),
    {"Description": "A beautiful sunset over the ocean"}
);

test_partial_deserializer!(
    test_anyof_with_markdown_partial,
    r#"```json
{"Description": "Test"#,
    baml_tyannotated!(Inspiration),
    anyof_db(),
    {"Description": "Test"}
);

fn response_db() -> TypeRefDb<'static, &'static str> {
    baml_db! {
        class Response {
            content: (string | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
        }
    }
}

test_partial_deserializer!(
    test_nested_anyof_no_leak,
    r#"```json
{"content": "[json"#,
    baml_tyannotated!(Response),
    response_db(),
    {"content": "[json"}
);

test_partial_deserializer!(
    test_anyof_with_nested_incomplete,
    r#"{"content": "test value with {"#,
    baml_tyannotated!(Response),
    response_db(),
    {"content": "test value with {"}
);

// ============================================================================
// Section 16: Null handling in new system
//
// Types need explicit `<type> | null` - plain types don't allow null.
// ============================================================================

fn strict_null_db() -> TypeRefDb<'static, &'static str> {
    baml_db! {
        class StrictClass {
            required_str: string @class_in_progress_field_missing(never),
            optional_str: (string | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
        }
    }
}

// required_str field with class_in_progress_field_missing(never) cannot be null
test_partial_none_deserializer!(
    test_strict_null_required_missing,
    r#"{"optional_str": "hel"#,
    baml_tyannotated!(StrictClass @in_progress(never)),
    strict_null_db()
);

// Both fields present
test_partial_deserializer!(
    test_strict_null_both_present,
    r#"{"required_str": "hello", "optional_str": "wor"#,
    baml_tyannotated!(StrictClass),
    strict_null_db(),
    {"required_str": "hello", "optional_str": "wor"}
);

// Complete with null optional
test_deserializer!(
    test_strict_null_complete_with_null_optional,
    r#"{"required_str": "hello", "optional_str": null}"#,
    baml_tyannotated!(StrictClass),
    strict_null_db(),
    {"required_str": "hello", "optional_str": null}
);

// ============================================================================
// Section 17: StreamState combined with class_in_progress_field_missing
// ============================================================================

fn stream_state_class_db() -> TypeRefDb<'static, &'static str> {
    baml_db! {
        class StatusReport {
            title: string @class_in_progress_field_missing(null),
            progress: (int | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
        }
    }
}

// StreamState on a class: incomplete class still gets state
test_partial_deserializer!(
    test_stream_state_class_incomplete,
    r#"{"title": "Build"#,
    baml_tyannotated!(StreamState<StatusReport>),
    stream_state_class_db(),
    {"value": {"title": "Build", "progress": null}, "state": "Incomplete"}
);

test_deserializer!(
    test_stream_state_class_complete,
    r#"{"title": "Build", "progress": 100}"#,
    baml_tyannotated!(StreamState<StatusReport>),
    stream_state_class_db(),
    {"value": {"title": "Build", "progress": 100}, "state": "Complete"}
);

// ============================================================================
// Section 18: Multiple StreamState fields in a class
// ============================================================================

fn multi_stream_state_db() -> TypeRefDb<'static, &'static str> {
    baml_db! {
        class MultiStream {
            name: StreamState<string> @class_in_progress_field_missing(null),
            count: StreamState<int> @class_in_progress_field_missing(null),
            label: (string | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
        }
    }
}

test_partial_deserializer!(
    test_multi_stream_state_partial,
    r#"{"name": "hel"#,
    baml_tyannotated!(MultiStream),
    multi_stream_state_db(),
    {"name": {"value": "hel", "state": "Incomplete"}, "count": null, "label": null}
);

test_deserializer!(
    test_multi_stream_state_complete,
    r#"{"name": "hello", "count": 5, "label": "done"}"#,
    baml_tyannotated!(MultiStream),
    multi_stream_state_db(),
    {"name": {"value": "hello", "state": "Complete"}, "count": {"value": 5, "state": "Complete"}, "label": "done"}
);

// ============================================================================
// Section 19: Edge cases - empty objects, deeply nested, etc.
// ============================================================================

fn edge_case_db() -> TypeRefDb<'static, &'static str> {
    baml_db! {
        class Leaf {
            value: (string | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
        }
        class Branch {
            leaf: (Leaf | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
            children: [Branch] @class_in_progress_field_missing([]),
        }
    }
}

// Empty incomplete object
test_partial_deserializer!(
    test_edge_empty_incomplete_object,
    r#"{"#,
    baml_tyannotated!(Branch),
    edge_case_db(),
    {"leaf": null, "children": []}
);

// Deeply nested partial
test_partial_deserializer!(
    test_edge_deeply_nested_partial,
    r#"{"leaf": {"value": "root"}, "children": [{"leaf": {"value": "child"}, "children": [{"leaf": {"value": "grandch"#,
    baml_tyannotated!(Branch),
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
    baml_tyannotated!(Branch),
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
// Section 20: Mixed in_progress + class_in_progress_field_missing + StreamState
// ============================================================================

fn mixed_all_db() -> TypeRefDb<'static, &'static str> {
    baml_db! {
        class MixedItem {
            id: int @class_in_progress_field_missing(never),
            name: StreamState<string> @class_in_progress_field_missing(null),
            tags: [string] @class_in_progress_field_missing([]),
        }
    }
}

// id missing => class excluded
test_partial_none_deserializer!(
    test_mixed_all_id_missing,
    r#"{"name": "tes"#,
    baml_tyannotated!(MixedItem @in_progress(never)),
    mixed_all_db()
);

// id present, name is StreamState incomplete
test_partial_deserializer!(
    test_mixed_all_id_present_partial,
    r#"{"id": 1, "name": "tes"#,
    baml_tyannotated!(MixedItem),
    mixed_all_db(),
    {"id": 1, "name": {"value": "tes", "state": "Incomplete"}, "tags": []}
);

// All complete
test_deserializer!(
    test_mixed_all_complete,
    r#"{"id": 1, "name": "test", "tags": ["a", "b"]}"#,
    baml_tyannotated!(MixedItem),
    mixed_all_db(),
    {"id": 1, "name": {"value": "test", "state": "Complete"}, "tags": ["a", "b"]}
);

// List of MixedItem with in_progress(never): incomplete items filtered
test_partial_deserializer!(
    test_mixed_all_list_filtered,
    r#"[{"id": 1, "name": "done", "tags": ["x"]}, {"id": 2, "name": "par"#,
    baml_tyannotated!([MixedItem @in_progress(never)]),
    mixed_all_db(),
    [{"id": 1, "name": {"value": "done", "state": "Complete"}, "tags": ["x"]}]
);
