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

// ─── code-point escapes: \xHH, \uHHHH, \u{...} (JavaScript semantics) ───

function hex_escape() -> string {
    "a\x41b"
}

function unicode_escape() -> string {
    "a\u0042b"
}

function braced_unicode_escape() -> string {
    "\u{1F600}"
}

function surrogate_pair_escape() -> string {
    "\uD83D\uDE00"
}

function backtick_code_point_escapes() -> string {
    `\x41\u0042\u{43}`
}

test "code point escapes decode in quoted and backtick strings" {
    assert.equal(hex_escape(), "aAb");
    assert.equal(hex_escape().length(), 3);
    assert.equal(unicode_escape(), "aBb");
    assert.equal(unicode_escape().length(), 3);
    assert.equal(braced_unicode_escape(), "😀");
    assert.equal(braced_unicode_escape().length(), 1);
    assert.equal(surrogate_pair_escape(), "😀");
    assert.equal(backtick_code_point_escapes(), "ABC");
    assert.equal("\x1b[0m".length(), 4);
    assert.equal("\x1b", string.from_code_points([27]));
    assert.equal("\xe9", "é");
    assert.equal("\u00e9\u{E9}", "éé");
    assert.equal("it\'s", "it's")
}

test "malformed code point escapes are compile errors" {
    root.compiler.AssertHasCode({ "main.baml": `function f() -> string { "\\x4" }` }, "E0176");
    root.compiler.AssertHasCode({ "main.baml": `function f() -> string { "\\u12" }` }, "E0176");
    root.compiler.AssertHasCode({ "main.baml": `function f() -> string { "\\u{110000}" }` }, "E0176");
    root.compiler.AssertHasCode({ "main.baml": `function f() -> string { "\\uD800" }` }, "E0176");
    root.compiler.AssertHasCode({ "main.baml": "function f() -> string { `\\u{}` }" }, "E0176")
}

test "unknown escapes are kept verbatim with a warning" {
    let files = { "main.baml": `function f() -> string { "a\\qb" }` };
    let found = false;
    for (let diagnostic in root.compiler.CompileSuccessDiagnostics(files)) {
        if (diagnostic.code == "E0176" && diagnostic.severity == "warning") {
            found = true;
        }
    }
    assert.is_true(found);
    root.compiler.AssertSuccessLacksMessage({ "main.baml": `function f() -> string { "\\x41\\u{41}" }` }, "invalid")
}
