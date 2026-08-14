// Tests for union type handling in BexExternalValue.
//
// Converted from crates/baml_tests/tests/typed_outputs.rs.
//
// The Rust tests assert on BexExternalValue::union(...) which captures the full
// union wrapper metadata (union variant types + selected branch type). BAML's
// assert.equal sees the concrete unwrapped value, not the union
// wrapper metadata. Tests are converted to verify the concrete result is correct.
// Tests that exist SOLELY to verify the union wrapper metadata structure are
// SKIPPED + FLAGGED below.
//
// SKIPPED/FLAGGED:
//   - union_int_or_string_returns_int: the Rust assertion checks
//     BexExternalValue::union(...) wrapper metadata specifically. The concrete
//     value (42) is testable, but the union metadata (selected_branch, union
//     variant list) cannot be observed from BAML. Converted to test concrete
//     value only — same for all union* tests.
//   - optional_union_preserves_union_metadata: comment says "Regression:
//     maybe_wrap_union early-returned for Ty::Optional" — this specifically
//     tests an internal representation detail (union wrapper preserved through
//     Optional). Only the concrete value 42 is testable in BAML.
//   - union_of_array_with_union_elements_or_string: comment says "Tests that
//     selected_option uses declared type, not inferred from values" — internal
//     selected_option field, not observable from BAML.
//   - array_of_unions: elements are BexExternalValue::union(...) per-element;
//     BAML cannot observe per-element union wrapper metadata.
//   - map_with_union_values: same — per-value union wrapper metadata.

// ─── simple scalar results ────────────────────────────────────────────────────

function union_int_or_string_int_main() -> int | string {
    42
}

test "union_int_or_string_returns_int" {
    assert.equal(union_int_or_string_int_main(), 42)
}

function union_int_or_string_string_main() -> int | string {
    "hello"
}

test "union_int_or_string_returns_string" {
    assert.equal(union_int_or_string_string_main(), "hello")
}

function optional_int_value_main() -> int? {
    42
}

test "optional_int_returns_value" {
    assert.equal(optional_int_value_main(), 42)
}

function optional_int_null_main() -> int? {
    null
}

test "optional_int_returns_null" {
    assert.equal(optional_int_null_main(), null)
}

// ─── class results ────────────────────────────────────────────────────────────

class Response {
    data int | string
}

function class_with_union_field_main() -> Response {
    Response { data: 42 }
}

test "class_with_union_field" {
    assert.equal(class_with_union_field_main(), Response { data: 42 })
}

class Success {
    value int
}

class Failure {
    error string
}

function union_classes_success_main() -> Success | Failure {
    Success { value: 42 }
}

test "union_of_classes_returns_success" {
    assert.equal(union_classes_success_main(), Success { value: 42 })
}

function union_classes_failure_main() -> Success | Failure {
    Failure { error: "something went wrong" }
}

test "union_of_classes_returns_failure" {
    assert.equal(union_classes_failure_main(), Failure { error: "something went wrong" })
}

// ─── array results ────────────────────────────────────────────────────────────

function union_of_arrays_main() -> int[] | string[] {
    [1, 2, 3]
}

test "union_of_arrays" {
    assert.equal(union_of_arrays_main(), [1, 2, 3])
}

// array_of_unions: (int | string)[] — each element is a union-wrapped value.
// The concrete values are [1, "two", 3]; BAML can verify that.
function array_of_unions_main() -> (int | string)[] {
    [1, "two", 3]
}

test "array_of_unions" {
    assert.equal(array_of_unions_main(), [1, "two", 3])
}

// ─── optional class results ───────────────────────────────────────────────────

class OutputData {
    value int
}

function optional_class_main() -> OutputData? {
    OutputData { value: 42 }
}

// optional_class: OutputData? result — `!` unwrap is not a BAML operator, so
// access a field via a helper to produce a non-optional int for comparison.
function optional_class_value() -> int {
    let result = optional_class_main();
    match (result) {
        OutputData => result.value,
        null => -1
    }
}

test "optional_class" {
    assert.equal(optional_class_value(), 42)
}

function optional_class_null_main() -> OutputData? {
    null
}

test "optional_class_returns_null" {
    assert.equal(optional_class_null_main(), null)
}

class Person {
    name string
    age int?
}

function class_with_optional_field_main() -> Person {
    Person { name: "Alice", age: null }
}

test "class_with_optional_field" {
    assert.equal(class_with_optional_field_main(), Person { name: "Alice", age: null })
}

// ─── optional union ───────────────────────────────────────────────────────────

function optional_union_value_main() -> (int | string)? {
    42
}

test "optional_union_preserves_union_metadata" {
    // The Rust test verifies union wrapper metadata internals; here we verify
    // only the observable concrete value.
    assert.equal(optional_union_value_main(), 42)
}

function optional_union_null_main() -> (int | string)? {
    null
}

test "optional_union_returns_null" {
    assert.equal(optional_union_null_main(), null)
}

// ─── map with union values ────────────────────────────────────────────────────

function map_with_union_values_main() -> map<string, int | string> {
    {"count": 42, "name": "test"}
}

// map_with_union_values: the per-value union-wrapper metadata (which branch was
// selected) isn't observable from BAML, so we compare the concrete entries.
test "map_with_union_values" {
    assert.equal(map_with_union_values_main(), {"count": 42, "name": "test"})
}

// ─── union of array with union elements or string ─────────────────────────────

function union_of_array_with_union_elements_main() -> (int | bool)[] | string {
    [1, true, 2]
}

test "union_of_array_with_union_elements_or_string" {
    // The Rust test checks that selected_option uses declared type (internal
    // metadata). Here we verify the concrete array value is correct.
    assert.equal(union_of_array_with_union_elements_main(), [1, true, 2])
}
