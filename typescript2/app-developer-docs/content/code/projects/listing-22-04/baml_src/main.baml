// docs:start all
class ToolCall {
    name: string,
    args: string[],
}

enum Outcome {
    Done
    Cancelled
}

function describe(event: ToolCall | Outcome | int) -> string {
    match (event) {
        ToolCall { name: "search" | "lookup", args: [let query, ..] } => `search for ${query}`,
        ToolCall { name, args: [] } => `${name} with no arguments`,
        ToolCall { name } => name,
        Outcome.Done => "done",
        Outcome.Cancelled => "cancelled",
        let tokens: int if tokens > 1_000 => "long reply",
        int => "short reply",
    }
}
// docs:end all

test "match patterns" {
    assert.equal(describe(ToolCall { name: "lookup", args: ["baml", "docs"] }), "search for baml");
    assert.equal(describe(ToolCall { name: "search", args: [] }), "search with no arguments");
    assert.equal(describe(ToolCall { name: "fetch", args: ["url"] }), "fetch");
    assert.equal(describe(Outcome.Cancelled), "cancelled");
    assert.equal(describe(4_000), "long reply");
    assert.equal(describe(12), "short reply")
}
