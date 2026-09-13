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
