// Tests for baml.json.parse, stringify, and stringify_pretty.
//
// Converted from crates/baml_tests/tests/json_parse_stringify.rs.
//
// Tests that read parsed results through a `let` local before doing work on
// them are moved to helper functions to avoid the test-block local-boxing
// VM bug.
//
// `stringify_pretty_multiline`: The Rust test checks s.contains('\n'). BAML has
// string.includes, so we call it directly on the stringify_pretty result.
//
// `parse_throws_on_garbage`: checks that parsing "{[" raises baml.json.ParseError.
// `roundtrip_nested_object`: uses a string local, moved to helper to avoid boxing.

// ─── 2.1 Parse an array of ints ──────────────────────────────────────────────

// The result is a json array of 3 elements. We verify length via match in a helper.
function parse_array_len() -> int {
    let j: json = baml.json.parse("[1, 2, 3]");
    match (j) {
        let arr: json[] => arr.length()
        _ => -1
    }
}

test "parse_array_of_ints" {
    assert.equal(parse_array_len(), 3)
}

// ─── 2.2 Parse then match: array arm ─────────────────────────────────────────

function parse_then_match_array_len() -> int {
    let j: json = baml.json.parse("[1, 2, 3]")
    match (j) {
        let arr: json[] => arr.length()
        _ => -1
    }
}

test "parse_then_match_array" {
    assert.equal(parse_then_match_array_len(), 3)
}

// ─── 2.3 Roundtrip: stringify(parse(s)) ──────────────────────────────────────

test "roundtrip_stringify_parse" {
    assert.equal(baml.json.stringify(baml.json.parse("[1,2,3]")), "[1,2,3]")
}

// ─── 2.4 Int/float disambiguation ────────────────────────────────────────────

function parse_is_int() -> bool {
    match (baml.json.parse("1")) {
        let n: int => true
        _ => false
    }
}

test "parse_int_disambiguation" {
    assert.is_true(parse_is_int())
}

function parse_is_float() -> bool {
    match (baml.json.parse("1.0")) {
        let f: float => true
        _ => false
    }
}

test "parse_float_disambiguation" {
    assert.is_true(parse_is_float())
}

// ─── 2.5 Parse throws on garbage input ───────────────────────────────────────

test "parse_throws_on_garbage" {
    {
        baml.json.parse("{[");
        baml.sys.panic("expected parse to throw")
    } catch (e) {
        baml.json.ParseError => null
    }
}

// ─── 2.6 stringify_pretty ────────────────────────────────────────────────────

// The Rust test checks s.contains('\n'). BAML has string.includes, so we call
// it directly on the stringify_pretty result.
test "stringify_pretty_multiline" {
    assert.is_true(baml.json.stringify_pretty(baml.json.parse("{\"a\":1}")).includes("\n"))
}

// ─── 2.7 Roundtrip a nested object ───────────────────────────────────────────

// The string `s` is in a local inside main; moved to helper to avoid boxing.
function roundtrip_nested() -> string {
    let s: string = "{\"x\":1,\"y\":[2,3]}"
    baml.json.stringify(baml.json.parse(s))
}

test "roundtrip_nested_object" {
    assert.equal(roundtrip_nested(), "{\"x\":1,\"y\":[2,3]}")
}
