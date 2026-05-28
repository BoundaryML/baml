// Tests for `Object::Type` structural equality via `baml.llm.get_return_type`.
//
// Converted from crates/baml_tests/tests/type_reflection.rs.
//
// `baml.llm.get_return_type(name)` is a pure static lookup — it does NOT
// make an actual LLM network call. The client block is required by the
// compiler (LLM functions must declare a client) but no request is sent.
//
// Each test body is lifted into a top-level helper function to avoid the
// test-block local-boxing VM bug (locals holding `type` values misbehave
// when compared inside a test block).

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

function eq_type_values_same_fn() -> bool {
    let a = baml.llm.get_return_type("GetFoo");
    let b = baml.llm.get_return_type("GetFoo");
    a == b
}

test "eq_type_values_same" {
    assert.is_true(baml.deep_equals(eq_type_values_same_fn(), true))
}

function eq_type_values_different_fn() -> bool {
    let a = baml.llm.get_return_type("GetFoo");
    let b = baml.llm.get_return_type("GetBar");
    a == b
}

test "eq_type_values_different" {
    assert.is_true(baml.deep_equals(eq_type_values_different_fn(), false))
}

function neq_type_values_same_fn() -> bool {
    let a = baml.llm.get_return_type("GetFoo");
    let b = baml.llm.get_return_type("GetFoo");
    a != b
}

test "neq_type_values_same" {
    assert.is_true(baml.deep_equals(neq_type_values_same_fn(), false))
}

function neq_type_values_different_fn() -> bool {
    let a = baml.llm.get_return_type("GetFoo");
    let b = baml.llm.get_return_type("GetBar");
    a != b
}

test "neq_type_values_different" {
    assert.is_true(baml.deep_equals(neq_type_values_different_fn(), true))
}

function deep_equals_type_values_same_fn() -> bool {
    let a = baml.llm.get_return_type("GetFoo");
    let b = baml.llm.get_return_type("GetFoo");
    baml.deep_equals(a, b)
}

test "deep_equals_type_values_same" {
    assert.is_true(baml.deep_equals(deep_equals_type_values_same_fn(), true))
}

function deep_equals_type_values_different_fn() -> bool {
    let a = baml.llm.get_return_type("GetFoo");
    let b = baml.llm.get_return_type("GetBar");
    baml.deep_equals(a, b)
}

test "deep_equals_type_values_different" {
    assert.is_true(baml.deep_equals(deep_equals_type_values_different_fn(), false))
}
