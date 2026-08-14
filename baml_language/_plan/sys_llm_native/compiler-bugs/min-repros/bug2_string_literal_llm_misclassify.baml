// BUG 2: a bare string literal containing "client"/"prompt" made the parser
// classify a plain function as an LLM function (pre-fix: E0010 cascade at
// CHECK time — this file would not compile).
function bug2() -> string {
    "the client sent nothing"
}

test "bug2_compiles_and_runs" {
    assert.equal(bug2(), "the client sent nothing")
}
