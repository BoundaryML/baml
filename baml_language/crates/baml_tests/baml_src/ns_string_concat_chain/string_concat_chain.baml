// A chain of string `+`, and so every template, joins its parts in one step.
// These tests pin that the result is the same string `+` builds one part at a
// time: the text, its length in chars, and the order the parts run in.

class ConcatLog {
    entries: string[],
}

function logged(log: ConcatLog, part: string) -> string {
    log.entries.push(part);
    part
}

function request_line(method: string, item: string, page: int, verbose: bool) -> string {
    `${method} /v1/items/${item}?page=${page}&verbose=${verbose}`
}

function plus_line(method: string, item: string, page: int) -> string {
    method + " /v1/items/" + item + "?page=" + page.to_string()
}

test "chains_build_the_same_text" {
    assert.equal(request_line("GET", "a1", 2, true), "GET /v1/items/a1?page=2&verbose=true");
    assert.equal(plus_line("PUT", "b2", -3), "PUT /v1/items/b2?page=-3");
    let a = "a";
    let empty = "";
    assert.equal(empty + a + empty + "b" + empty, "ab");
    assert.equal(`${empty}${empty}${empty}`, "")
}

test "chains_keep_unicode_lengths" {
    let s = "héllo " + "😀" + " 世界" + "!";
    assert.equal(s, "héllo 😀 世界!");
    assert.equal(s.length(), 11);
    assert.equal(s.at(6), "😀");
    let long = "é".repeat(30) + "-" + "x".repeat(30) + "😀";
    assert.equal(long.length(), 62);
    assert.equal(long.at(30), "-");
    assert.equal(long.at(61), "😀")
}

test "chains_join_long_parts" {
    let big = "x".repeat(5000);
    let s = "<" + big + "|" + big + ">";
    assert.equal(s.length(), 10003);
    assert.equal(s.slice(0, 3), "<xx");
    assert.equal(s.slice(5000, 5003), "x|x");
    assert.equal(s.slice(10000, 10003), "xx>")
}

test "append_loops_build_the_same_text" {
    let s = "";
    let t = "";
    let i = 0;
    while (i < 2000) {
        s = s + "," + i.to_string();
        t = `${t},${i}`;
        i += 1;
    }
    assert.equal(s, t);
    assert.equal(s.length(), 8890);
    assert.equal(s.slice(0, 8), ",0,1,2,3");
    assert.equal(s.slice(8885, 8890), ",1999")
}

test "chains_run_parts_left_to_right" {
    let log = ConcatLog { entries: [] };
    let s = logged(log, "a") + logged(log, "b") + "-" + logged(log, "c") + logged(log, "d");
    assert.equal(s, "ab-cd");
    assert.equal(log.entries, ["a", "b", "c", "d"]);
    let t = `${logged(log, "e")}/${logged(log, "f")}/${logged(log, "g")}`;
    assert.equal(t, "e/f/g");
    assert.equal(log.entries, ["a", "b", "c", "d", "e", "f", "g"])
}
