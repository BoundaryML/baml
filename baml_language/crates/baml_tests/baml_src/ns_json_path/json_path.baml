// Unit tests for `baml.json.path<T>` — the jq-style typed JSON accessor.
//
// Test bodies live in top-level helper functions (not inlined in test blocks) per the
// convention in this suite: reflection/runtime-type behavior applied to boxed test-block
// locals can misreport; function locals are fine.

class PathPerson {
    name: string,
    age: int,
}

function _doc() -> json throws never {
    baml.json
        .parse(
            "{\"usage\":{\"input_tokens\":12,\"output_tokens\":5},\"choices\":[{\"message\":{\"content\":\"hi\"}},{\"message\":{\"content\":\"second\"}}],\"by_name\":{\"anna.k\":{\"id\":7}},\"person\":{\"name\":\"Ada\",\"age\":36},\"flag\":true,\"score\":0.5}",
        ) catch (e) {
        _ => null,
    }
}

// ── happy paths ───────────────────────────────────────────────────────────────

function path_int_field() -> int {
    baml.json.path<int>(_doc(), ".usage.input_tokens") catch (e) {
        _ => -1,
    }
}

test "path: .usage.input_tokens -> int" {
    assert.is_true(baml.deep_equals(path_int_field(), 12))
}

function path_nested_array_string() -> string {
    baml.json.path<string>(_doc(), ".choices[1].message.content") catch (e) {
        _ => "ERR",
    }
}

test "path: .choices[1].message.content -> string" {
    assert.is_true(baml.deep_equals(path_nested_array_string(), "second"))
}

function path_quoted_key_with_dot() -> int {
    baml.json.path<int>(_doc(), ".by_name[\"anna.k\"].id") catch (e) {
        _ => -1,
    }
}

test "path: quoted bracket key containing a dot" {
    assert.is_true(baml.deep_equals(path_quoted_key_with_dot(), 7))
}

function path_single_quoted_key() -> int {
    baml.json.path<int>(_doc(), ".by_name['anna.k'].id") catch (e) {
        _ => -1,
    }
}

test "path: single-quoted bracket key" {
    assert.is_true(baml.deep_equals(path_single_quoted_key(), 7))
}

function path_root_array_index() -> int {
    let arr = baml.json.parse("[10, 20, 30]") catch (e) {
        _ => null,
    };
    baml.json.path<int>(arr, "[1]") catch (e) {
        _ => -1,
    }
}

test "path: [1] on a root array" {
    assert.is_true(baml.deep_equals(path_root_array_index(), 20))
}

function path_jq_identity_then_index() -> int {
    let arr = baml.json.parse("[10, 20, 30]") catch (e) {
        _ => null,
    };
    baml.json.path<int>(arr, ".[2]") catch (e) {
        _ => -1,
    }
}

test "path: .[2] (jq identity-then-index) on a root array" {
    assert.is_true(baml.deep_equals(path_jq_identity_then_index(), 30))
}

function path_identity_to_class() -> string {
    let p: PathPerson = baml.json.path<PathPerson>(_doc(), ".person") catch (e) {
        _ => {
            return "ERR";
        },
    };
    p.name + "|" + p.age.to_string()
}

test "path: leaf coerces to a user class" {
    assert.is_true(baml.deep_equals(path_identity_to_class(), "Ada|36"))
}

function path_bool_and_float() -> string {
    let b = baml.json.path<bool>(_doc(), ".flag") catch (e) {
        _ => false,
    };
    let f = baml.json.path<float>(_doc(), ".score") catch (e) {
        _ => -1.0,
    };
    b.to_string() + "|" + f.to_string()
}

test "path: bool and float leaves" {
    assert.is_true(baml.deep_equals(path_bool_and_float(), "true|0.5"))
}

function path_full_chain() -> string {
    let j = baml.json.parse("{\"foo\":{\"hi\":{\"bar\":[{\"name\":\"deep\"}]}}}") catch (e) {
        _ => null,
    };
    baml.json.path<string>(j, ".foo[\"hi\"].bar[0].name") catch (e) {
        _ => "ERR",
    }
}

test "path: .foo[\"hi\"].bar[0].name chained access" {
    assert.is_true(baml.deep_equals(path_full_chain(), "deep"))
}

// ── error paths (each throws JsonPathError) ──────────────────────────────────

function _path_err(selector: string) -> string {
    let v: int = baml.json.path<int>(_doc(), selector) catch (e) {
        let pe: baml.json.JsonPathError => {
            return pe.message;
        },
    };
    "no-error:" + v.to_string()
}

test "path error: missing field" {
    assert.is_true(_path_err(".nope").starts_with("missing field 'nope'"))
}

test "path error: index out of bounds" {
    assert.is_true(_path_err(".choices[9].message").starts_with("index [9] out of bounds"))
}

test "path error: index on a non-array" {
    assert.is_true(_path_err(".usage[0]").starts_with("index [0] on a non-array"))
}

test "path error: field access on a non-object" {
    assert.is_true(_path_err(".flag.deeper").starts_with("field access '.deeper' on a non-object"))
}

test "path error: bad bracket token" {
    assert.is_true(_path_err(".choices[abc]").starts_with("bad bracket token '[abc]'"))
}

test "path error: unterminated bracket" {
    assert.is_true(_path_err(".choices[0").starts_with("unterminated '['"))
}

test "path error: bare selector (must be jq style)" {
    assert.is_true(_path_err("usage.input_tokens").starts_with("selector must start with"))
}

test "path error: leaf does not coerce" {
    // .person is an object; asking for int at the leaf must throw, not misparse.
    assert.is_true(_path_err(".person").starts_with("leaf does not coerce"))
}

// ── path_or (the coalescing sibling) ─────────────────────────────────────────

function path_or_hit() -> int {
    baml.json.path_or<int>(_doc(), ".usage.input_tokens", -1)
}

function path_or_missing() -> int {
    baml.json.path_or<int>(_doc(), ".usage.nonexistent", -1)
}

function path_or_mismatch() -> string {
    // .person is an object; the string coercion fails -> default.
    baml.json.path_or<string>(_doc(), ".person", "none")
}

function path_or_bad_selector() -> string {
    baml.json.path_or<string>(_doc(), "not-a-selector", "fallback")
}

function path_or_nested_hit() -> string {
    baml.json.path_or<string>(_doc(), ".choices[1].message.content", "")
}

test "path_or: present value wins over the default" {
    assert.is_true(baml.deep_equals(path_or_hit(), 12))
}

test "path_or: missing field yields the default, never throws" {
    assert.is_true(baml.deep_equals(path_or_missing(), -1))
}

test "path_or: uncoercible leaf yields the default" {
    assert.equal(path_or_mismatch(), "none")
}

test "path_or: malformed selector yields the default" {
    assert.equal(path_or_bad_selector(), "fallback")
}

test "path_or: nested selector hit" {
    assert.equal(path_or_nested_hit(), "second")
}

function path_or_inferred() -> string {
    // T infers from the default argument — no explicit type args needed.
    baml.json.path_or(_doc(), ".choices[0].message.content", "")
}

test "path_or: type argument infers from the default" {
    assert.equal(path_or_inferred(), "hi")
}
