// scenario 12 — tool taxonomy: ../../../../../llm-provider/ideas/scenarios/12-tool-taxonomy/README.md
// Execution-location is a marker on the tool.

enum ToolKind {
    InProcess,
    Hosted,
    Mcp,
    ComputerUse,
}

class KindedTool {
    tool: baml.ai.Tool,
    kind: ToolKind,
}

function describe_tool_kind(kt: KindedTool) -> string throws never {
    match (kt.kind) {
        ToolKind.InProcess => "runs in this process",
        ToolKind.Hosted => "runs provider-side",
        ToolKind.Mcp => "runs on an MCP server",
        ToolKind.ComputerUse => "operates a computer",
    }
}

test "12: the taxonomy marker drives dispatch-side description" {
    let t = baml.ai.Tool { name: "browse", description: "d", parameters: "{}" };
    assert.equal(describe_tool_kind(KindedTool { tool: t, kind: ToolKind.Mcp }), "runs on an MCP server")
    assert.equal(describe_tool_kind(KindedTool { tool: t, kind: ToolKind.InProcess }), "runs in this process")
}
