// Tests for the `json` type alias.
//
// Converted from crates/baml_tests/tests/json_alias.rs.
//
// `field_of_type_json_lowers_cleanly`: The Rust test checks `class_name == "user.Foo"` via
// Instance pattern match — moved to a helper function to avoid the test-block local-boxing
// VM bug (reading an Instance field from a local throws `expected Instance, got Any`).
// The helper returns the `data` field (null); we compare via assert.equal since it's optional.

class Foo { data json }

// Helper: returns the `data` field from a freshly-constructed Foo.
function foo_data() -> json {
    Foo { data: null }.data
}

// ─── 1.1 Class field of type json ────────────────────────────────────────────

test "field_of_type_json_lowers_cleanly" {
    // Foo { data: null } should have data == null.
    assert.equal(foo_data(), null)
}

// ─── 1.2 Union arm: null ─────────────────────────────────────────────────────

test "let_json_null" {
    assert.equal(null, null)
}

// ─── 1.3 Union arm: int ──────────────────────────────────────────────────────

test "let_json_int" {
    assert.equal(42, 42)
}

// ─── 1.4 Union arm: string ───────────────────────────────────────────────────

test "let_json_string" {
    assert.equal("hello", "hello")
}

// ─── 1.5 Union arm: array ────────────────────────────────────────────────────

// Result is a json array of length 3. We use a helper to avoid local-boxing when
// reading back through a `let`.
function json_array_len() -> int {
    let arr: json = [1, 2, 3];
    match (arr) {
        let a: json[] => a.length()
        _ => -1
    }
}

test "let_json_array" {
    assert.equal(json_array_len(), 3)
}

// ─── 1.6 Union arm: map literal ──────────────────────────────────────────────

// Check that a json map with key "key" round-trips through a match.
// We verify the map has exactly 1 entry (and that it typed as map<string,json>).
function json_map_has_key() -> bool {
    let m: json = {"key": "value"};
    match (m) {
        let obj: map<string, json> => obj.length() == 1
        _ => false
    }
}

test "let_json_map" {
    assert.is_true(json_map_has_key())
}

class OptionalJsonField {
    data json?
}

function nested_map_in_optional_json_field() -> bool {
    let value = OptionalJsonField {
        data: { chat_template_kwargs: { enable_thinking: false } },
    };
    baml.json.path<bool>(value.data, ".chat_template_kwargs.enable_thinking")
}

test "nested_map_literal_in_optional_json_field" {
    assert.is_true(!nested_map_in_optional_json_field())
}

// ─── 1.7 Exhaustive match against json ───────────────────────────────────────

// The function takes `json` as a parameter; passing `null` should return "null".
// All eight arms are covered.
function match_json(j: json) -> string {
    match (j) {
        null => "null",
        let b: bool => "bool",
        let i: int => "int",
        let bi: bigint => "bigint",
        let f: float => "float",
        let s: string => "string",
        let arr: json[] => "array",
        let obj: map<string, json> => "map"
    }
}

test "match_json_arms_exhaustive" {
    assert.equal(match_json(null), "null")
}

// ─── 1.8 Impl dispatch with `json` as a generic argument (B-1078) ────────────

// The impl head below carries the recursive alias as a generic argument. The
// value's recorded type args cross the normalizer on their way into bytecode,
// so runtime dispatch compares the impl head's spelling of `JsonWrap<json>`
// against the normalized one — before equirecursive canonicalization these
// could disagree (alias vs unfolding) and the resolver declined an impl the
// compiler had selected.

interface JsonTagged {
    function tag(self) -> string throws never
}

class JsonWrap<T> {
    data T
}

implements JsonTagged for JsonWrap<json> {
    function tag(self) -> string { return "json-wrap" }
}

function json_wrap_tag() -> string {
    let w = JsonWrap<json> { data: null }
    let t: JsonTagged = w
    return t.tag()
}

test "impl_on_recursive_alias_generic_arg_dispatches" {
    assert.equal(json_wrap_tag(), "json-wrap")
}

// ─── 1.9 Container arms on a json scrutinee ──────────────────────────────────

// `json[]` and `map<string, json>` are json's only list and map members, so a
// match on a json value tests these arms by the runtime tag alone. Every kind
// of json value must still reach its own arm, also through recursion (the
// request builders prune nulls this way).

function json_kind(j: json) -> string {
    match (j) {
        let object: map<string, json> => "map",
        let items: json[] => "list",
        _ => "scalar",
    }
}

function json_kinds() -> string[] {
    [
        json_kind({"a": 1}),
        json_kind({}),
        json_kind([1, "two"]),
        json_kind([]),
        json_kind("map"),
        json_kind(1),
        json_kind(true),
        json_kind(null),
    ]
}

test "json_container_arms_match_each_kind" {
    assert.equal(json_kinds(), ["map", "map", "list", "list", "scalar", "scalar", "scalar", "scalar"])
}

function drop_nulls(j: json) -> json {
    match (j) {
        let object: map<string, json> => {
            let out: map<string, json> = {};
            for (let key in object.keys()) {
                let entry = object.get(key);
                match (entry) {
                    null => null,
                    _ => {
                        let _ = out.set(key, drop_nulls(entry));
                        null
                    },
                };
            }
            out
        },
        let items: json[] => {
            let out: json[] = [];
            for (let item in items) {
                out.push(drop_nulls(item));
            }
            out
        },
        _ => j,
    }
}

function drop_nulls_nested() -> string {
    baml.json.stringify(drop_nulls(baml.json.parse("{\"a\":null,\"b\":[1,{\"c\":null,\"d\":\"x\"}],\"e\":{\"f\":null}}")))
}

test "json_container_arms_recurse_through_nested_values" {
    assert.equal(drop_nulls_nested(), "{\"b\":[1,{\"d\":\"x\"}],\"e\":{}}")
}
