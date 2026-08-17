// Internal helper behind `ai.mcp.McpConnection.tools()`. Nothing here is part
// of the public ai surface.

// A tools/call proxy bound to one tool name. A helper function rather than
// an inline closure in the tools() loop: a closure in a loop captures the
// loop variable's slot, so every tool would proxy the last catalog entry;
// a parameter is a fresh binding per call.
function _proxy(
    conn: root.mcp.McpConnection,
    name: string,
) -> (map<string, unknown>) -> string throws unknown {
    (args: map<string, unknown>) -> {
        conn.call_tool(name, args)
    }
}
