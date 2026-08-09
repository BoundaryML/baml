// Live smoke: MCP tools as ordinary BAML tools (root.mcp), run with the
// Claude Code client — the same harness the harness-native MCP path uses,
// but here every MCP call is a journaled ToolRequested/ToolCompleted pair
// riding the outcome envelope. No API key; npx fetches the server.
//   ../target/debug/baml-cli run -e 'live_mcp_tools()'

function live_mcp_tools() -> string {
    let conn = root.mcp.McpConnection.connect(
        "everything",
        "npx",
        ["-y", "@modelcontextprotocol/server-everything"],
        timeout_ms = 300000,
    );
    defer {
        conn.close()
    }
    let spec = echo_spec(root.claude_code.ClaudeCodeClient.new(), conn.tools());
    let result = root.ai.Agent<EchoBack>.new(on_event = log_event).run(spec);
    let completed = result.journal.entries()
        .filter((e: root.ai.Event) -> {
            match (e) {
                let c: root.ai.ToolCompleted => true,
                _ => false,
            }
        })
        .length();
    `reply=${result.value.reply} tool_completed_events=${completed}`
}
