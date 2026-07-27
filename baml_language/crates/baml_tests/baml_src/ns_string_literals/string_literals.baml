function escaped_newline() -> string {
    "a\nb"
}

function lone_newline() -> string {
    "\n"
}

function escaped_tab() -> string {
    "a\tb"
}

function escaped_cr() -> string {
    "a\rb"
}

function escaped_backslash() -> string {
    "a\\b"
}

function escaped_quote() -> string {
    "a\"b"
}

function escaped_newline_length() -> int {
    "a\nb".length()
}

function lone_newline_length() -> int {
    "\n".length()
}

function escaped_tab_length() -> int {
    "a\tb".length()
}

function escaped_backslash_length() -> int {
    "a\\b".length()
}

function escaped_quote_length() -> int {
    "a\"b".length()
}

function json_body() -> string {
    "{\"input\":\"hello\\nworld\",\"model\":\"m\"}"
}

function raw_keeps_backslash_n() -> string {
    #"a\nb"#
}

function raw_keeps_quote() -> string {
    ##"""##
}

function lone_backslash() -> string {
    "\\"
}

function lone_backslash_length() -> int {
    "\\".length()
}

function double_backslash() -> string {
    "\\\\"
}

function double_backslash_length() -> int {
    "\\\\".length()
}

function trailing_double_backslash() -> string {
    "a\\\\"
}

function trailing_double_backslash_length() -> int {
    "a\\\\".length()
}

function replace_backslash() -> string {
    "a\\b\\c".replace_all("\\", "/")
}

function raw_double_backslash() -> string {
    #"\\"#
}

function raw_quad_backslash() -> string {
    #"\\\\"#
}

function json_body_is_parseable_fn() -> bool {
    let parsed = baml.json.from_string<map<string, string>>(json_body());
    parsed["input"].to_utf8().to_hex() == "68656c6c6f0a776f726c64" && parsed["model"] == "m"
}

test "quoted_string_escape_sequences_decode_to_exact_bytes" {
    assert.equal(escaped_newline().to_utf8().to_hex(), "610a62");
    assert.equal(lone_newline().to_utf8().to_hex(), "0a");
    assert.equal(escaped_tab().to_utf8().to_hex(), "610962");
    assert.equal(escaped_cr().to_utf8().to_hex(), "610d62");
    assert.equal(escaped_backslash().to_utf8().to_hex(), "615c62");
    assert.equal(escaped_quote().to_utf8().to_hex(), "612262")
}

test "quoted_string_escape_lengths_match" {
    assert.equal(escaped_newline_length(), 3);
    assert.equal(lone_newline_length(), 1);
    assert.equal(escaped_tab_length(), 3);
    assert.equal(escaped_backslash_length(), 3);
    assert.equal(escaped_quote_length(), 3)
}

test "json_body_is_parseable_by_serde" {
    assert.is_true(json_body_is_parseable_fn())
}

test "raw_strings_do_not_decode_escapes" {
    assert.equal(raw_keeps_backslash_n().to_utf8().to_hex(), "615c6e62");
    assert.equal(raw_keeps_quote().to_utf8().to_hex(), "22")
}

test "escaped_backslash_at_string_boundary" {
    assert.equal(lone_backslash().to_utf8().to_hex(), "5c");
    assert.equal(double_backslash().to_utf8().to_hex(), "5c5c");
    assert.equal(trailing_double_backslash().to_utf8().to_hex(), "615c5c")
}

test "escaped_backslash_boundary_lengths" {
    assert.equal(lone_backslash_length(), 1);
    assert.equal(double_backslash_length(), 2);
    assert.equal(trailing_double_backslash_length(), 3)
}

test "replace_all_with_backslash_arguments" {
    assert.equal(replace_backslash(), "a/b/c")
}

test "raw_strings_preserve_backslash_sequences" {
    assert.equal(raw_double_backslash().to_utf8().to_hex(), "5c5c");
    assert.equal(raw_quad_backslash().to_utf8().to_hex(), "5c5c5c5c")
}
