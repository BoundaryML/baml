// Small deterministic helpers.

function i2s(n: int) -> string {
    baml.unstable.string(n)
}

// Render a float with 2 decimals (no format mini-language on this toolchain).
function f2(x: float) -> string {
    let rounded = (x * 100.0).iround() catch (_) {
        _ => 0
    };
    let whole = rounded / 100;
    let frac = rounded - whole * 100;
    let fa = if (frac < 0) {
        0 - frac
    } else {
        frac
    };
    let fs = if (fa < 10) {
        "0" + i2s(fa)
    } else {
        i2s(fa)
    };
    i2s(whole) + "." + fs
}

// Truncate to at most n characters, appending a marker when cut.
function clip(s: string, n: int) -> string {
    if (s.length() <= n) {
        s
    } else {
        s.substring(0, n) + "\n…[truncated]"
    }
}

// Keep the most recent n characters (the useful end of a transcript).
function recent(s: string, n: int) -> string {
    if (s.length() <= n) {
        s
    } else {
        "[earlier turns omitted]\n" + s.substring(s.length() - n, s.length())
    }
}

// Left-pad with spaces to width n (table rendering).
function pad(s: string, n: int) -> string {
    let out = s;
    while (out.length() < n) {
        out = " " + out;
    }
    out
}

// Right-pad with spaces to width n.
function rpad(s: string, n: int) -> string {
    let out = s;
    while (out.length() < n) {
        out = out + " ";
    }
    out
}

function command_text(out: baml.sys.ShellOutput) -> string {
    "exit "
        + i2s(out.exit_code)
        + "\nstdout:\n"
        + clip(out.stdout.to_string(), 6000)
        + "\nstderr:\n"
        + clip(out.stderr.to_string(), 2000)
}

test "clip and recent" {
    assert.equal(clip("abcdef", 3), "abc\n…[truncated]");
    assert.equal(clip("abc", 3), "abc");
    assert.equal(recent("abcdef", 3), "[earlier turns omitted]\ndef");
    assert.equal(recent("ab", 3), "ab");
}

test "padding" {
    assert.equal(pad("x", 3), "  x");
    assert.equal(rpad("x", 3), "x  ");
    assert.equal(pad("xyzw", 3), "xyzw");
}

test "f2 formatting" {
    assert.equal(f2(1.5), "1.50");
    assert.equal(f2(0.056), "0.06");
    assert.equal(f2(12.0), "12.00");
}
