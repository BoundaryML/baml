// Regression tests for string character access and iteration.
//
// Tests that use locals (`let` bindings, compound assignment, `for` loops)
// go through a top-level helper function to avoid the test-block local-boxing
// VM bug.

// ─── helpers ─────────────────────────────────────────────────────────────────

// split("") on a multi-byte string must yield exactly one entry per character,
// with no empty-string padding at the ends.
function strregr_split_empty_delimiter() -> string {
    let chars = "aé😀".split("");
    baml.json.stringify(chars.length()) + ":" + chars[0] + chars[1] + chars[2]
}

// Strings iterate by character (not byte), including multi-byte code points.
function strregr_iterate_by_character() -> string {
    let out = "";
    for (let c in "aé😀") {
        out += "[" + c + "]";
    }
    out
}

// .chars() returns one array element per character.
function strregr_chars_array() -> string {
    let chars = "aé😀".chars();
    baml.json.stringify(chars.length()) + ":" + chars[0] + chars[1] + chars[2]
}

// ─── tests ───────────────────────────────────────────────────────────────────

test "split_empty_delimiter_has_no_padding" {
    assert.equal(strregr_split_empty_delimiter(), "3:aé😀")
}

test "split_empty_string_returns_empty_array" {
    assert.equal("".split("").length(), 0)
}

test "string_is_iterable_by_character" {
    assert.equal(strregr_iterate_by_character(), "[a][é][😀]")
}

test "chars_returns_character_array" {
    assert.equal(strregr_chars_array(), "3:aé😀")
}
