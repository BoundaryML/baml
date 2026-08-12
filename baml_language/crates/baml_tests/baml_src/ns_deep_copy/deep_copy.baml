// Unified tests for `baml.deep_copy` and structural equality (`==`).
//
// Converted from crates/baml_tests/tests/deep_copy.rs.
//
// Tests that mutate locals + do nested field access are wrapped in top-level
// helper functions to avoid the test-block local-boxing VM bug
// (`expected Instance, got Any` when accessing fields of a value read from a
// let local in a test block).
//
// Bytecode snapshot assertions are ignored — they test compiler internals, not
// runtime behaviour.

// ============================================================================
// deep_copy tests
// ============================================================================

class Tree {
    value string
    children Tree[]
}

// deep_copy_object: result is structural, compare with `assert.equal`.
function deep_copy_object_helper() -> Tree {
    let t = Tree { value: "1", children: [
        Tree { value: "2", children: [] },
        Tree { value: "3", children: [] },
    ] };
    let copy = baml.deep_copy(t);
    copy
}

test "deep_copy_object" {
    assert.equal(
        deep_copy_object_helper(),
        Tree { value: "1", children: [ Tree { value: "2", children: [] }, Tree { value: "3", children: [] }] },
    )
}

// ── deep_copy_independence ────────────────────────────────────────────────────

class Node {
    value int
    children Node[]
}

// Mutate the original after copying; the copy's nested value must stay at 2.
function deep_copy_independence_helper() -> int {
    let original = Node { value: 1, children: [
        Node { value: 2, children: [] },
        Node { value: 3, children: [] },
    ] };
    let copy = baml.deep_copy(original);
    original.children[0].value = 99;
    copy.children[0].value
}

test "deep_copy_independence" {
    assert.equal(deep_copy_independence_helper(), 2)
}

// ── deep_copy_nested_arrays_in_class ─────────────────────────────────────────

class Matrix {
    data int[][]
}

function deep_copy_nested_arrays_helper() -> int {
    let original = Matrix { data: [[1, 2], [3, 4]] };
    let copy = baml.deep_copy(original);
    original.data[0][0] = 99;
    copy.data[0][0]
}

test "deep_copy_nested_arrays_in_class" {
    assert.equal(deep_copy_nested_arrays_helper(), 1)
}

// ── deep_copy_map_in_class ───────────────────────────────────────────────────

class Container {
    values map<string, int>
}

function deep_copy_map_helper() -> int {
    let original = Container {
        values: {"a": 1, "b": 2}
    };
    let copy = baml.deep_copy(original);
    original.values["a"] = 99;
    copy.values["a"]
}

test "deep_copy_map_in_class" {
    assert.equal(deep_copy_map_helper(), 1)
}

// ── deep_copy_complex_nested_structure ───────────────────────────────────────

class DeepInner {
    value int
}

class DeepMiddle {
    inner DeepInner
    list DeepInner[]
}

class DeepOuter {
    middle DeepMiddle
    data map<string, DeepInner>
}

// Modify multiple parts of the original; copy must return sum of original
// values: 1 + 2 + 4 = 7.
function deep_copy_complex_helper() -> int {
    let original = DeepOuter {
        middle: DeepMiddle {
            inner: DeepInner { value: 1 },
            list: [DeepInner { value: 2 }, DeepInner { value: 3 }]
        },
        data: {
            "first": DeepInner { value: 4 },
            "second": DeepInner { value: 5 }
        }
    };
    let copy = baml.deep_copy(original);
    original.middle.inner.value = 100;
    original.middle.list[0].value = 200;
    original.data["first"].value = 300;
    copy.middle.inner.value + copy.middle.list[0].value + copy.data["first"].value
}

test "deep_copy_complex_nested_structure" {
    assert.equal(deep_copy_complex_helper(), 7)
}

// ── deep_copy_circular_reference ─────────────────────────────────────────────

// Circular reference: a → [b] → [a]. Deep-copy then mutate original; copy.value
// should stay 1.
function deep_copy_circular_helper() -> int {
    let a = Node { value: 1, children: [] };
    let b = Node { value: 2, children: [a] };
    a.children = [b];
    let copy = baml.deep_copy(a);
    a.value = 99;
    copy.value
}

test "deep_copy_circular_reference" {
    assert.equal(deep_copy_circular_helper(), 1)
}

// ============================================================================
// structural equality (`==`) tests
// ============================================================================

// Primitives — both locals alias the same constant, so they compare equal.
function eq_primitives_helper() -> bool {
    let a = 42;
    let b = 42;
    a == b
}

test "eq_primitives" {
    assert.is_true(eq_primitives_helper())
}

// Different primitives — they compare unequal.
function eq_different_primitives_helper() -> bool {
    let a = 42;
    let b = 43;
    a == b
}

test "eq_different_primitives" {
    assert.is_true(!eq_different_primitives_helper())
}

// ── simple objects ────────────────────────────────────────────────────────────

class Point {
    x int
    y int
}

test "eq_simple_objects" {
    assert.equal(Point { x: 10, y: 20 }, Point { x: 10, y: 20 })
}

test "eq_different_objects" {
    assert.is_true(Point { x: 10, y: 20 } != Point { x: 10, y: 21 })
}

// ── nested objects ────────────────────────────────────────────────────────────

test "eq_nested_objects" {
    assert.equal(
        Node { value: 1, children: [ Node { value: 2, children: [] }, Node { value: 3, children: [] } ] },
        Node { value: 1, children: [ Node { value: 2, children: [] }, Node { value: 3, children: [] } ] },
    )
}

test "eq_nested_objects_different" {
    assert.is_true(Node { value: 1, children: [ Node { value: 2, children: [] }, Node { value: 3, children: [] } ] } != Node { value: 1, children: [ Node { value: 2, children: [] }, Node { value: 4, children: [] } ] })
}

// ── container with arrays ─────────────────────────────────────────────────────

class ArrayContainer {
    data int[]
}

test "eq_with_arrays" {
    assert.equal(ArrayContainer { data: [1, 2, 3, 4] }, ArrayContainer { data: [1, 2, 3, 4] })
}

// ── container with maps ───────────────────────────────────────────────────────

class MapContainer {
    values map<string, int>
}

test "eq_with_maps" {
    assert.equal(
        MapContainer { values: {"a": 1, "b": 2} },
        MapContainer { values: {"a": 1, "b": 2} },
    )
}

// ── same reference ────────────────────────────────────────────────────────────

function eq_same_reference_helper() -> bool {
    let n = Node { value: 1, children: [] };
    n == n
}

test "eq_same_reference" {
    assert.is_true(eq_same_reference_helper())
}

// ── circular structures ───────────────────────────────────────────────────────

function eq_circular_helper() -> bool {
    let a1 = Node { value: 1, children: [] };
    let b1 = Node { value: 2, children: [a1] };
    a1.children = [b1];

    let a2 = Node { value: 1, children: [] };
    let b2 = Node { value: 2, children: [a2] };
    a2.children = [b2];

    a1 == a2
}

test "eq_circular_structure" {
    assert.is_true(eq_circular_helper())
}
