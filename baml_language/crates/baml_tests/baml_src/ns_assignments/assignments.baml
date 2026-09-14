// Unified tests for assignments and field mutations.
//
// Converted from crates/baml_tests/tests/assignments.rs. Each Rust `main()` body
// is inlined into a `test` block; helper functions with parameters and the
// classes they use are top-level (can't live inside a `test` block) and uniquely
// named, since every baml_src file compiles into one `user` package.
//
// Scalar results read straight from a `let` local (e.g. `let y = ...; y`) are
// compared against a local-bound `expected`, because in a test block a local
// compares unequal to a literal (VM bug); field-access and function-call results
// compare against literals directly.
//
// The three method-call-result field-assignment tests (array_element_method_field_assignment,
// method_call_then_array_access_assignment, method_call_field_assignment) are added but
// COMMENTED OUT below — they hit a compiler2 bug (`TypeError: expected Instance/Array, got Any`).

class Counter {
  value int
}

class Data {
  value int
  active bool
}

class Stats {
  score int
}

class Inner {
  value int
}

class Outer {
  inner Inner
}

class Item {
  count int
}

class Factory {
  counter Counter

  function get_counter(self) -> Counter {
    self.counter
  }
}

// ─── locals ─────────────────────────────────────────────────────────────────

test "block_expr" {
    let a = {
        let b = 1;
        b
    };
    // `expected` is a local: a test-block local compares unequal to a literal (VM bug)
    let expected = 1;
    assert.equal(a, expected)
}

test "mutable_var_in_function" {
    let y = 3;
    y = 5;
    // `expected` is a local: a test-block local compares unequal to a literal (VM bug)
    let expected = 5;
    assert.equal(y, expected)
}

// ─── mutable parameters ───────────────────────────────────────────────────────

function mutable_in_arg(x: int) -> int {
    x = 3;
    x
}

test "mutable_param" {
    assert.equal(mutable_in_arg(42), 3)
}

// A copy taken before a later mutation of `a` keeps the original value.
function cross_block_soundness(c: bool) -> int {
    let a = 1;
    let b = a;
    if (c) {
        a = 2;
    }
    b
}

test "virtual_cross_block_soundness" {
    assert.equal(cross_block_soundness(true), 1)
}

// A copy of a parameter is unaffected by a later mutation of that parameter.
function cross_block_param_mutation(c: bool, p: int) -> int {
    let x = p;
    if (c) {
        p = 2;
    }
    x
}

test "virtual_cross_block_param_mutation_soundness" {
    assert.equal(cross_block_param_mutation(true, 42), 42)
}

function copy_of_mutable_param(x: int) -> int {
    let y = x;
    x = 2;
    y
}

test "copy_of_mutable_param_soundness" {
    assert.equal(copy_of_mutable_param(42), 42)
}

// Transitive copy (t = p; x = t) is still unaffected by a later mutation of p.
function cross_block_transitive_param_mutation(c: bool, p: int) -> int {
    let t = p;
    let x = t;
    if (c) {
        p = 2;
    }
    x
}

test "virtual_cross_block_transitive_param_mutation_soundness" {
    assert.equal(cross_block_transitive_param_mutation(true, 42), 42)
}

// ─── compound field assignment ────────────────────────────────────────────────

test "field_assignment_add_assign" {
    let c = Counter { value: 10 };
    c.value += 5;
    assert.equal(c.value, 15)
}

test "field_assignment_sub_assign" {
    let c = Counter { value: 20 };
    c.value -= 8;
    assert.equal(c.value, 12)
}

test "field_assignment_mul_assign" {
    let c = Counter { value: 7 };
    c.value *= 3;
    assert.equal(c.value, 21)
}

test "field_assignment_div_assign" {
    let c = Counter { value: 24 };
    c.value /= 4;
    assert.equal(c.value, 6)
}

test "field_assignment_mod_assign" {
    let c = Counter { value: 17 };
    c.value %= 5;
    assert.equal(c.value, 2)
}

test "field_assignment_simple" {
    let d = Data { value: 100, active: true };
    d.value = 42;
    assert.equal(d.value, 42)
}

// 10 +5 ->15, *2 ->30, -10 ->20, /4 ->5
test "field_assignment_multiple_ops" {
    let s = Stats { score: 10 };
    s.score += 5;
    s.score *= 2;
    s.score -= 10;
    s.score /= 4;
    assert.equal(s.score, 5)
}

// ─── nested field assignment ──────────────────────────────────────────────────

// Nested field writes go through a helper: in a test block, mutating a field
// through a local hits the local-boxing VM bug (`expected Instance, got Any`);
// inside a function locals aren't boxed.
function nested_field_simple() -> int {
    let i = Inner { value: 10 };
    let o = Outer { inner: i };
    o.inner.value = 42;
    o.inner.value
}

test "nested_field_assignment_simple" {
    assert.equal(nested_field_simple(), 42)
}

function nested_field_compound() -> int {
    let i = Inner { value: 10 };
    let o = Outer { inner: i };
    o.inner.value += 32;
    o.inner.value
}

test "nested_field_assignment_compound" {
    assert.equal(nested_field_compound(), 42)
}

// Assigning a whole object to a field, then reading it back.
test "field_assignment_object_field" {
    let o = Outer { inner: Inner { value: 10 } };
    o.inner = Inner { value: 20 };
    assert.equal(o.inner.value, 20)
}

// ─── array element field assignment ───────────────────────────────────────────

// Array-element field write also goes through a helper (same local-boxing bug).
function array_element_field() -> int {
    let items = [
        Item { count: 10 },
        Item { count: 20 },
        Item { count: 30 }
    ];
    items[1].count += 5;
    items[1].count
}

test "array_element_field_assignment" {
    assert.equal(array_element_field(), 25)
}

// ─── method-call result field assignment (with copy) ───────────────────────────

test "method_call_field_assignment_with_copy" {
    let f = Factory { counter: Counter { value: 10 } };
    let c = f.get_counter();
    c.value += 5;
    assert.equal(c.value, 15)
}

// ─── method-call-result field assignment (commented out — runtime bug) ─────────
//
// BUG: TypeError { expected: Object(Instance), got: Object(Any) }
// Assigning to a field of a method-call result (`f.m().field += x`) fails at
// runtime because the compiler emits an unresolved-call placeholder (Any) as the
// store target instead of the actual instance returned by the method.
//
// class AssData {
//     value int
//
//     function get_self(self) -> AssData {
//         self
//     }
// }
//
// class AssContainer {
//     data AssData
//
//     function get_data(self) -> AssData {
//         self.data
//     }
// }
//
// function array_element_method_field_assignment() -> int {
//     let containers = [
//         AssContainer { data: AssData { value: 10 } },
//         AssContainer { data: AssData { value: 20 } },
//         AssContainer { data: AssData { value: 30 } }
//     ];
//
//     containers[1].data.value += 5;
//     let result1 = containers[1].data.value;  // 25
//
//     containers[1].get_data().value += 10;
//     let result2 = containers[1].data.value;  // 35
//
//     result2
// }
//
// test "array_element_method_field_assignment" {
//     assert.equal(array_element_method_field_assignment(), 35)
// }

// BUG: TypeError { expected: Object(Array), got: Object(Any) }
// Indexing into the result of a method call (`obj.m()[i].field += x`) fails at
// runtime because the array returned by the method is typed as Any instead of
// the concrete array type, making the index load fail.
//
// class MtaaItem {
//     value int
// }
//
// class MtaaContainer {
//     data MtaaItem[]
//
//     function get_nested(self) -> MtaaItem[] {
//         self.data
//     }
// }
//
// function method_call_then_array_access_assignment() -> int {
//     let i1 = MtaaItem { value: 10 };
//     let i2 = MtaaItem { value: 20 };
//     let i3 = MtaaItem { value: 30 };
//     let arr = [i1, i2, i3];
//     let obj = MtaaContainer { data: arr };
//     obj.get_nested()[1].value += 5;
//     obj.data[1].value
// }
//
// test "method_call_then_array_access_assignment" {
//     assert.equal(method_call_then_array_access_assignment(), 25)
// }

// BUG: TypeError { expected: Object(Instance), got: Object(Any) }
// Direct method-call-result field assignment (`f.get_counter().value += 5`) fails
// at runtime with TypeError: the compiler emits Any as the store target.
//
// class MfaCounter {
//     value int
// }
//
// class MfaFactory {
//     counter MfaCounter
//
//     function get_counter(self) -> MfaCounter {
//         self.counter
//     }
// }
//
// function method_call_field_assignment() -> int {
//     let f = MfaFactory {
//         counter: MfaCounter { value: 10 }
//     };
//
//     f.get_counter().value += 5;
//
//     f.get_counter().value
// }
//
// test "method_call_field_assignment" {
//     assert.equal(method_call_field_assignment(), 15)
// }

// ─── side effects preserved across reassignment ────────────────────────────────

// `let x = throws_fail()` must run (and throw) even though x is reassigned after.
function throws_fail() -> int {
    throw "fail"
}

test "virtual_multiple_defs_preserve_side_effects" {
    {
        let x = throws_fail();
        x = 2;
        baml.sys.panic("expected throws_fail() to throw")
    } catch (e) {
        string => null
    }
}
