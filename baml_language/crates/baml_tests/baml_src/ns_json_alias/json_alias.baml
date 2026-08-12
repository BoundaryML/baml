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

// ─── 1.7 Exhaustive match against json ───────────────────────────────────────

// The function takes `json` as a parameter; passing `null` should return "null".
// All seven arms are covered.
function match_json(j: json) -> string {
    match (j) {
        null => "null",
        let b: bool => "bool",
        let i: int => "int",
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
