// docs:start all
class ToolCall {
    name: string,
    args: string[],
}

function single_query(call: ToolCall) -> string {
    if let ToolCall { name: "search", args: [let query] } = call {
        query
    } else {
        "none"
    }
}
// docs:end all

test "if let" {
    assert.equal(single_query(ToolCall { name: "search", args: ["baml"] }), "baml");
    assert.equal(single_query(ToolCall { name: "search", args: ["a", "b"] }), "none");
    assert.equal(single_query(ToolCall { name: "fetch", args: ["baml"] }), "none")
}
