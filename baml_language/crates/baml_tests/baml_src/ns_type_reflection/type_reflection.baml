// Tests for `Object::Type` structural equality via `ai.internal.get_return_type`.
//
// Converted from crates/baml_tests/tests/type_reflection.rs.
//
// `ai.internal.get_return_type(name)` is a pure static lookup — it does NOT
// make an actual LLM network call. The client declaration is required by the
// compiler (LLM functions must declare a client) but no request is sent.
//
// The direct test-block comparisons below also guard the local-boxing fix.

client TestClient = openai.ResponsesClient.new(
    model = "gpt-4o-mini",
    api_key = "test-key",
    base_url = "http://localhost:1234",
);

class Foo { x int }
class Bar { y string }

function GetFoo() -> Foo {
    client: TestClient
    prompt: `hi`
}

function GetBar() -> Bar {
    client: TestClient
    prompt: `hi`
}

// ─── Equality / inequality tests ─────────────────────────────────────────────

function eq_type_values_same_fn() -> bool {
    let a = ai.internal.get_return_type("GetFoo");
    let b = ai.internal.get_return_type("GetFoo");
    a == b
}

test "eq_type_values_same" {
    assert.is_true(eq_type_values_same_fn())
}

function eq_type_values_different_fn() -> bool {
    let a = ai.internal.get_return_type("GetFoo");
    let b = ai.internal.get_return_type("GetBar");
    a == b
}

test "eq_type_values_different" {
    assert.is_true(!eq_type_values_different_fn())
}

function neq_type_values_same_fn() -> bool {
    let a = ai.internal.get_return_type("GetFoo");
    let b = ai.internal.get_return_type("GetFoo");
    a != b
}

test "neq_type_values_same" {
    assert.is_true(!neq_type_values_same_fn())
}

function neq_type_values_different_fn() -> bool {
    let a = ai.internal.get_return_type("GetFoo");
    let b = ai.internal.get_return_type("GetBar");
    a != b
}

test "neq_type_values_different" {
    assert.is_true(neq_type_values_different_fn())
}

// BEP-066: all type equality sites use the canonical mint, so union order is
// irrelevant in both function and test-block contexts.
function union_order_eq_fn() -> bool {
    type.of<int | string>() == type.of<string | int>()
}

test "union_order_eq_canonical_in_function" {
    assert.is_true(union_order_eq_fn())
}

test "union_order_eq_canonical_in_test_block" {
    let a = type.of<int | string>();
    let b = type.of<string | int>();
    assert.is_true(a == b)
}
