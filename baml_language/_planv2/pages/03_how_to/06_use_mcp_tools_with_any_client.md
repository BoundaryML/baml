# Use MCP tools with any client

The `root.mcp` library speaks the MCP protocol itself and projects a
server's catalog into ordinary `Tool` values. The runner executes the
calls, so each one is journaled as `ToolRequested` and `ToolCompleted`,
visible to `on_event`, and governed by the run's tool-failure policy —
with every client, provider APIs and Claude Code alike.

```baml
let conn: McpConnection = mcp.McpConnection.connect(
    "everything",
    "npx",
    ["-y", "@modelcontextprotocol/server-everything"],
);
defer { conn.close() }

let spec: FunctionSpec<EchoBack> = ...;   // toolbox: Toolbox.new(conn.tools())
let result: RunResult<EchoBack> = ai.Agent<EchoBack>.new().run(spec);
```

`connect` spawns the server as a child process over stdio and performs
the initialize handshake; the connection lives until `close()`, and the
caller owns the lifetime. `tools()` lists the catalog and builds one
`Tool` per entry with `ai.raw_tool` — an MCP `inputSchema` is already
JSON Schema, so it carries into `Tool.input_schema` unchanged — whose
handler proxies `tools/call` over the connection.

A result's text content items join as the tool's output. An `isError`
result throws inside the handler, so the runner journals `ToolFailed`
and the model sees the failure, like any tool error
(`../02_guides/01_functions/02_tools.md`).

Offline, a fake MCP server is a shell script that answers the
handshake, the catalog, and the calls over stdio; the reference tree's
`tests/mcp.baml` tests the connection end-to-end this way, with no
network and no server install.

Relative to harness-native attachment
(`05_attach_mcp_servers_to_claude_code.md`), this form records every
call and works across clients; the harness form keeps multi-call
episodes inside one turn and its calls off the journal. The journaled
form is canonical, and the harness form is a per-client optimization
(`../05_appendix/02_alternatives_considered.md`).
