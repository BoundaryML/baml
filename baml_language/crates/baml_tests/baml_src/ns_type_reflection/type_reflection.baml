// Tests for BEP-066 minted `Object::Type` identity via
// `baml.llm.get_return_type`.
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

// ─── BEP-066 PR 4: unified type equality on permuted unions ─────────────────
//
// PR 1 deliberately pinned the old divergence: `==` canonicalized while
// `baml.deep_equals` compared the `RealizedTy` payload syntactically. BEP-066
// slice-1 PR 4 flips those pins: all three VM equality sites compare the same
// mint. Static mints digest the canonical form, so union order is irrelevant
// to both operations. Function and test-block contexts remain paired as the
// regression net for #3782.

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
    // Flipped in BEP-066 slice-1 PR 4: deep_equals now uses the same static
    // canonical mint as `==`.
    assert.is_true(baml.deep_equals(union_order_deep_equals_fn(), true))
}

test "union_order_deep_equals_syntactic_in_test_block" {
    let a = type.of<int | string>();
    let b = type.of<string | int>();
    assert.is_true(baml.deep_equals(baml.deep_equals(a, b), true))
}
