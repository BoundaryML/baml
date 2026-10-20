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

function lone_backslash() -> string {
    "\\"
}

function double_backslash() -> string {
    "\\\\"
}

function trailing_double_backslash() -> string {
    "a\\\\"
}

function replace_backslash() -> string {
    "a\\b\\c".replace_all("\\", "/")
}

test "quoted string escapes decode to exact values and lengths" {
    assert.equal(escaped_newline(), "a\nb");
    assert.equal(escaped_newline().length(), 3);
    assert.equal(lone_newline(), "\n");
    assert.equal(lone_newline().length(), 1);
    assert.equal(escaped_tab(), "a\tb");
    assert.equal(escaped_tab().length(), 3);
    assert.equal(escaped_cr(), "a\rb");
    assert.equal(escaped_backslash(), "a\\b");
    assert.equal(escaped_backslash().length(), 3);
    assert.equal(escaped_quote(), "a\"b");
    assert.equal(escaped_quote().length(), 3)
}

test "escaped backslash boundaries preserve values and lengths" {
    assert.equal(lone_backslash(), "\\");
    assert.equal(lone_backslash().length(), 1);
    assert.equal(double_backslash(), "\\\\");
    assert.equal(double_backslash().length(), 2);
    assert.equal(trailing_double_backslash(), "a\\\\");
    assert.equal(trailing_double_backslash().length(), 3)
}

test "replace_all accepts backslash arguments" {
    assert.equal(replace_backslash(), "a/b/c")
}

test "invalid quoted strings still produce compiler errors" {
    let backslash_eof = `function f() -> string { "hello` + "\\";
    let escaped_quote_eof = `function f() -> string { "hello` + "\\" + `"}`;
    let unterminated = root.compiler.CompileDiagnostics({ "unterminated.baml": `function f() -> string { "hello }` });
    let trailing_backslash = root.compiler.CompileDiagnostics({ "backslash_eof.baml": backslash_eof });
    let escaped_quote = root.compiler.CompileDiagnostics({ "escaped_quote.baml": escaped_quote_eof });
    if (unterminated.length() == 0) {
        baml.sys.panic("unterminated quoted string compiled successfully")
    }
    if (trailing_backslash.length() == 0) {
        baml.sys.panic("quoted string ending in a backslash compiled successfully")
    }
    if (escaped_quote.length() == 0) {
        baml.sys.panic("escaped quote consumed the closing quote without a diagnostic")
    }
}

// ─── escape sequences ────────────────────────────────────────────────────────
//
// Escapes accept JSON string bodies as-is (except lone surrogates). Unknown or
// unsupported escapes are compile errors (E0109) in quoted strings, backtick
// literals, and byte strings.

function json_escapes() -> string {
    "\"\\\/\b\f\n\r\tA"
}

function quote_and_template_escapes() -> string {
    "it\'s \`ok\` \$5"
}

function nul_escape() -> string {
    "a\0b"
}

function nul_then_digit() -> string {
    "\01"
}

function hex_escape() -> string {
    "a\x41b\x7F"
}

function unicode_escapes() -> string {
    "é中\u{41}\u{1F600}\u{1F_600}\u{10FFFF}"
}

function backtick_code_point_escapes() -> string {
    `\x41B\u{43}`
}

test "JSON escapes decode" {
    assert.equal(json_escapes(), string.from_code_points([34, 92, 47, 8, 12, 10, 13, 9, 65]));
    assert.equal("https:\/\/x.com", "https://x.com")
}

test "quote and template-delimiter escapes decode" {
    assert.equal(quote_and_template_escapes(), "it's `ok` $5")
}

test "nul escape is never octal" {
    assert.equal(nul_escape(), string.from_code_points([97, 0, 98]));
    assert.equal(nul_then_digit(), string.from_code_points([0, 49]))
}

test "hex escapes decode ASCII" {
    assert.equal(hex_escape(), string.from_code_points([97, 65, 98, 127]));
    assert.equal("\x1b[0m".length(), 4)
}

test "unicode escapes decode" {
    assert.equal(
        unicode_escapes(),
        string.from_code_points([233, 20013, 65, 128512, 128512, 1114111]),
    );
    assert.equal(backtick_code_point_escapes(), "ABC");
    assert.equal("\u{4_1}", "A")
}

test "surrogate pairs combine in any form" {
    assert.equal("😀", "😀");
    assert.equal("\uD83D\u{DE00}", "😀");
    assert.equal("\u{D83D}\uDE00", "😀");
    assert.equal("\u{D83D}\u{DE00}", "😀");
    assert.equal(`😀`.length(), 1)
}

test "malformed hex and unicode escapes are compile errors" {
    root.compiler.AssertHasMessage(
        { "main.baml": `function f() -> string { "\\xE9" }` },
        "out of range `\\xE9` escape",
    );
    root.compiler.AssertHasMessage(
        { "main.baml": `function f() -> string { "\\x4" }` },
        "expected exactly two hex digits",
    );
    root.compiler.AssertHasMessage(
        { "main.baml": `function f() -> string { "\\u12" }` },
        "invalid `\\u` escape",
    );
    root.compiler.AssertHasMessage(
        { "main.baml": `function f() -> string { "\\u{110000}" }` },
        "value exceeds U+10FFFF",
    );
    root.compiler.AssertHasMessage(
        { "main.baml": `function f() -> string { "\\u{_41}" }` },
        "`_` is only allowed between hex digits",
    );
    root.compiler.AssertHasMessage(
        { "main.baml": `function f() -> string { "\\u{41_}" }` },
        "`_` is only allowed between hex digits",
    );
    root.compiler.AssertHasMessage(
        { "main.baml": "function f() -> string { `\\u{}` }" },
        "expected at least one hex digit",
    )
}

test "unpaired surrogates are compile errors" {
    root.compiler.AssertHasMessage(
        { "main.baml": `function f() -> string { "\\uD83D" }` },
        "unpaired UTF-16 surrogate U+D83D",
    );
    root.compiler.AssertHasMessage(
        { "main.baml": `function f() -> string { "\\u{DE00}" }` },
        "unpaired UTF-16 surrogate U+DE00",
    );
    root.compiler.AssertHasMessage(
        { "main.baml": `function f() -> string { "\\uD83Dx" }` },
        "unpaired UTF-16 surrogate U+D83D",
    )
}

test "Python unicode escapes are compile errors that suggest braced escapes" {
    root.compiler.AssertHasMessage(
        { "main.baml": `function f() -> string { "\\U0001F600" }` },
        "`\\U` escapes are not supported; use `\\u{...}`",
    );
    root.compiler.AssertHasMessage(
        { "main.baml": `function f() -> string { "\\N{GRINNING FACE}" }` },
        "`\\N{...}` escapes are not supported; use `\\u{...}`",
    )
}

test "unknown escapes are compile errors that suggest a literal backslash" {
    root.compiler.AssertRejected(
        { "main.baml": `function f() -> string { "\\d+" }` },
        root.compiler.ExactDiagnostic {
            code: "E0109",
            message: "unknown escape sequence `\\d`; write `\\\\d` for a literal backslash: unknown escape",
        },
    );
    for (let escape in ["\\v", "\\a", "\\e", "\\101", "\\q"]) {
        root.compiler.AssertHasCode({ "main.baml": `function f() -> string { "${escape}" }` }, "E0109");
        root.compiler.AssertHasCode({ "main.baml": `function f() -> string { \`${escape}\` }` }, "E0109")
    }
}

test "a backslash before a line break is not a line continuation" {
    root.compiler.AssertHasMessage(
        { "main.baml": "function f() -> string {\n  \"a\\\nb\"\n}" },
        "`\\` followed by a newline",
    );
    root.compiler.AssertHasMessage(
        { "main.baml": "function f() -> string {\n  `a\\\nb`\n}" },
        "`\\` followed by a newline",
    )
}

test "each invalid escape is reported once" {
    root.compiler.AssertMessageCount(
        { "main.baml": `function f() -> string { "\\d \\d \\x" }` },
        "escape",
        3,
    )
}

// ─── raw strings ─────────────────────────────────────────────────────────────

function raw_regex_pattern() -> string {
    r"\d+\.\w"
}

function raw_escaped_quote() -> string {
    r"a\"b"
}

function raw_code_point_escape() -> string {
    r"\u{41}"
}

function raw_empty() -> string {
    r""
}

test "raw strings keep backslashes verbatim" {
    assert.equal(raw_regex_pattern(), "\\d+\\.\\w");
    assert.equal(raw_escaped_quote(), "a\\\"b");
    assert.equal(raw_escaped_quote().length(), 4);
    assert.equal(raw_code_point_escape().length(), 6);
    assert.equal(raw_empty(), "");
    assert.equal(r"\n".length(), 2)
}

test "raw strings work as regex patterns" {
    assert.is_true(baml.regex.new(r"^\d+$").has_match("12345"));
    assert.equal("x1y22".replace_all(baml.regex.new(r"\d+"), "#"), "x#y#")
}

test "r followed by a space is not a raw string prefix" {
    root.compiler.AssertCompiles(
        { "main.baml": `function f() -> string { let r = "x"; r + "\\\\d" }` },
    )
}
