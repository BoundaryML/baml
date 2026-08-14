// Unified tests for class construction, field access, and methods.
//
// Converted from crates/baml_tests/tests/classes.rs. Instance-valued results and
// scalar field-access results from a local instance both use `assert.equal`
// (direct field access on the instance, not a plain local read).
// Tests requiring nested field writes go through top-level helpers to avoid the
// test-block local-boxing VM bug.

// ─── shared class definitions ─────────────────────────────────────────────────

class Point {
    x int
    y int
}

class MyClass {
    x int
    y int
}

class CInner {
    value int
}

class CInnerXY {
    x int
    y int
}

class COuter {
    inner CInner
}

class COuterXY {
    inner CInnerXY
    value int
}

class CInnerVal {
    val int
}

class COuterXI {
    inner CInnerVal
    x int
}

class Data {
    value int
}

class Counter {
    value int
}

class PointXYZW {
    x int
    y int
    z int
    w int
}

class Number {
    value int

    function add(self, other: Number) -> Number {
        Number { value: self.value + other.value }
    }
}

class MutableNumber {
    value int

    function add(self, other: MutableNumber) -> bool {
        self.value += other.value;
        true
    }
}

// ─── Construction ─────────────────────────────────────────────────────────────

function class_constructor_fn() -> Point {
    let p = Point { x: 1, y: 2 };
    p
}

test "class_constructor" {
    assert.equal(class_constructor_fn(), Point { x: 1, y: 2 })
}

function class_constructor_return_directly_fn() -> Point {
    Point { x: 1, y: 2 }
}

test "class_constructor_return_directly" {
    assert.equal(class_constructor_return_directly_fn(), Point { x: 1, y: 2 })
}

function constructor_with_preceding_variables_fn() -> int {
    let a = 10;
    let b = 20;
    let c = 30;
    let obj = MyClass { x: 100, y: 200 };
    obj.x + obj.y + a + b + c
}

test "constructor_with_preceding_variables" {
    assert.equal(constructor_with_preceding_variables_fn(), 360)
}

// ─── Nested construction ──────────────────────────────────────────────────────

function nested_construction_dead_store_fn() -> int {
    let o = COuter { inner: CInner { value: 42 } };
    42
}

test "nested_construction_dead_store" {
    assert.equal(nested_construction_dead_store_fn(), 42)
}

function nested_construction_field_access_fn() -> int {
    let o = COuterXY {
        inner: CInnerXY { x: 10, y: 20 },
        value: 30
    };
    o.value
}

test "nested_construction_field_access" {
    assert.equal(nested_construction_field_access_fn(), 30)
}

function nested_construction_inner_field_access_fn() -> int {
    let o = COuterXY {
        inner: CInnerXY { x: 10, y: 20 },
        value: 30
    };
    o.inner.y
}

test "nested_construction_inner_field_access" {
    assert.equal(nested_construction_inner_field_access_fn(), 20)
}

function nested_field_read_fn() -> int {
    let o = COuter { inner: CInner { value: 42 } };
    o.inner.value
}

test "nested_field_read" {
    assert.equal(nested_field_read_fn(), 42)
}

function nested_field_read_separate_construction_fn() -> int {
    let i = CInner { value: 42 };
    let o = COuter { inner: i };
    o.inner.value
}

test "nested_field_read_separate_construction" {
    assert.equal(nested_field_read_separate_construction_fn(), 42)
}

function nested_constructor_with_preceding_variables_fn() -> int {
    let a = 5;
    let b = 10;
    let obj = COuterXI {
        inner: CInnerVal { val: 100 },
        x: 50
    };
    obj.inner.val + obj.x + a + b
}

test "nested_constructor_with_preceding_variables" {
    assert.equal(nested_constructor_with_preceding_variables_fn(), 165)
}

// ─── Spread operator ──────────────────────────────────────────────────────────

function default_point() -> PointXYZW {
    PointXYZW { x: 0, y: 0, z: 0, w: 0 }
}

function x_one() -> PointXYZW {
    PointXYZW { x: 1, y: 0, z: 0, w: 0 }
}

function xy_one() -> PointXYZW {
    PointXYZW { x: 1, y: 1, z: 0, w: 0 }
}

function spread_before_named_fields_fn() -> PointXYZW {
    let p = PointXYZW { ...default_point(), x: 1, y: 2 };
    p
}

test "spread_before_named_fields" {
    assert.equal(spread_before_named_fields_fn(), PointXYZW { x: 1, y: 2, z: 0, w: 0 })
}

function spread_after_named_fields_fn() -> PointXYZW {
    let p = PointXYZW { x: 1, y: 2, ...default_point() };
    p
}

test "spread_after_named_fields" {
    assert.equal(spread_after_named_fields_fn(), PointXYZW { x: 0, y: 0, z: 0, w: 0 })
}

function multiple_spreads_fn() -> PointXYZW {
    let p = PointXYZW { ...x_one(), ...xy_one() };
    p
}

test "multiple_spreads" {
    assert.equal(multiple_spreads_fn(), PointXYZW { x: 1, y: 1, z: 0, w: 0 })
}

function spread_does_not_break_locals_fn() -> int {
    let p = PointXYZW { x: 1, y: 2, ...default_point() };
    let x = 0;
    x
}

test "spread_does_not_break_locals" {
    assert.equal(spread_does_not_break_locals_fn(), 0)
}

// ─── Field assignment ─────────────────────────────────────────────────────────

// These use helpers because nested field writes inside test blocks hit the
// local-boxing VM bug.

function field_assignment_fn() -> int {
    let d = Data { value: 0 };
    d.value = 42;
    d.value
}

test "field_assignment" {
    assert.equal(field_assignment_fn(), 42)
}

function field_compound_assignment_fn() -> int {
    let c = Counter { value: 5 };
    c.value += 10;
    c.value
}

test "field_compound_assignment" {
    assert.equal(field_compound_assignment_fn(), 15)
}

function nested_field_assignment_fn() -> int {
    let o = COuter { inner: CInner { value: 0 } };
    o.inner.value = 99;
    o.inner.value
}

test "nested_field_assignment" {
    assert.equal(nested_field_assignment_fn(), 99)
}

function nested_field_compound_assignment_fn() -> int {
    let o = COuter { inner: CInner { value: 5 } };
    o.inner.value += 10;
    o.inner.value
}

test "nested_field_compound_assignment" {
    assert.equal(nested_field_compound_assignment_fn(), 15)
}

// ─── Methods ──────────────────────────────────────────────────────────────────

function method_call_fn() -> int {
    let a = Number { value: 1 };
    let b = Number { value: 2 };
    let n = a.add(b);
    n.value
}

test "method_call" {
    assert.equal(method_call_fn(), 3)
}

function mutable_self_method_fn() -> int {
    let a = MutableNumber { value: 1 };
    let b = MutableNumber { value: 2 };
    a.add(b);
    a.value
}

test "mutable_self_method" {
    assert.equal(mutable_self_method_fn(), 3)
}
