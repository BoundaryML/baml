// Tests for `Object::Type` structural equality via `baml.llm.get_return_type`.
//
// Converted from crates/baml_tests/tests/type_reflection.rs.
//
// `baml.llm.get_return_type(name)` is a pure static lookup — it does NOT
// make an actual LLM network call. The client block is required by the
// compiler (LLM functions must declare a client) but no request is sent.
//
// The test bodies below deliberately keep their locals INSIDE the `test`
// blocks. They were originally lifted into top-level helper functions to
// dodge the "test-block local-boxing VM bug" (test-body `let` locals never
// resolved and lowered to `const null` — fixed by #3782, see
// thoughts/antonio/s1-vm-bug-diagnosis.md). Inlined bodies now double as
// the regression net for that fix.

client<llm> TestClient {
    provider openai
    options {
        model "gpt-4o-mini"
        api_key "test-key"
        base_url "http://localhost:1234"
    }
}

class Foo { x int }
class Bar { y string }

function GetFoo() -> Foo {
    client TestClient
    prompt #"hi"#
}

function GetBar() -> Bar {
    client TestClient
    prompt #"hi"#
}

// ─── Equality / inequality tests ─────────────────────────────────────────────

test "eq_type_values_same" {
    let a = baml.llm.get_return_type("GetFoo");
    let b = baml.llm.get_return_type("GetFoo");
    assert.is_true(baml.deep_equals(a == b, true))
}

test "eq_type_values_different" {
    let a = baml.llm.get_return_type("GetFoo");
    let b = baml.llm.get_return_type("GetBar");
    assert.is_true(baml.deep_equals(a == b, false))
}

test "neq_type_values_same" {
    let a = baml.llm.get_return_type("GetFoo");
    let b = baml.llm.get_return_type("GetFoo");
    assert.is_true(baml.deep_equals(a != b, false))
}

test "neq_type_values_different" {
    let a = baml.llm.get_return_type("GetFoo");
    let b = baml.llm.get_return_type("GetBar");
    assert.is_true(baml.deep_equals(a != b, true))
}

test "deep_equals_type_values_same" {
    let a = baml.llm.get_return_type("GetFoo");
    let b = baml.llm.get_return_type("GetFoo");
    assert.is_true(baml.deep_equals(baml.deep_equals(a, b), true))
}

test "deep_equals_type_values_different" {
    let a = baml.llm.get_return_type("GetFoo");
    let b = baml.llm.get_return_type("GetBar");
    assert.is_true(baml.deep_equals(baml.deep_equals(a, b), false))
}

// ─── CHARACTERIZATION: `==` vs `baml.deep_equals` on permuted unions ────────
//
// These tests PIN A KNOWN INCONSISTENCY — they are not an endorsement of it.
// BAML currently has divergent equality implementations for `type` values:
//
//   * `==` lowers through the `baml.ops.equals_equals` driver, which compares
//     type values with `vm.equivalent(..)` — CANONICAL equivalence, so union
//     member order does not matter (bex_vm/src/package_baml/ops.rs).
//   * `baml.deep_equals` compares with the derived `PartialEq` on `RealizedTy`
//     — SYNTACTIC equality, so union member order DOES matter
//     (bex_vm/src/package_baml/root.rs).
//
// Both behave identically in test-block and function contexts (the historical
// context divergence was #3782's lowering bug, now fixed). The follow-up
// s1-mint-identity PR replaces all equality sites with a single mint-identity
// comparison; when it lands these pins must be updated together with it.
// Full diagnosis: thoughts/antonio/s1-vm-bug-diagnosis.md.

function union_order_eq_fn() -> bool {
    // Same comparison as "union_order_eq_canonical_in_test_block", function context.
    type.of<int | string>() == type.of<string | int>()
}

test "union_order_eq_canonical_in_function" {
    // `==` : permuted unions compare EQUAL (canonical path).
    assert.is_true(baml.deep_equals(union_order_eq_fn(), true))
}

test "union_order_eq_canonical_in_test_block" {
    // Same comparison inline in a test block — must agree with the function
    // context (regression for the pre-#3782 context divergence).
    let a = type.of<int | string>();
    let b = type.of<string | int>();
    assert.is_true(baml.deep_equals(a == b, true))
}

function union_order_deep_equals_fn() -> bool {
    let a = type.of<int | string>();
    let b = type.of<string | int>();
    baml.deep_equals(a, b)
}

test "union_order_deep_equals_syntactic_in_function" {
    // `baml.deep_equals` : permuted unions compare UNEQUAL (syntactic path),
    // disagreeing with `==` above. Known bug, resolved by s1-mint-identity.
    assert.is_true(baml.deep_equals(union_order_deep_equals_fn(), false))
}

test "union_order_deep_equals_syntactic_in_test_block" {
    let a = type.of<int | string>();
    let b = type.of<string | int>();
    assert.is_true(baml.deep_equals(baml.deep_equals(a, b), false))
}
