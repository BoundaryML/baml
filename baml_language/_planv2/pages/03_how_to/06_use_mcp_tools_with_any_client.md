# Use MCP tools with any client

This page calls tools from an MCP server through the normal tool
loop, with any client.

An MCP server is a local process that provides tools over the Model
Context Protocol: JSON messages over its standard input and output.
The reference implementation ships `root.mcp`, a library that
connects to a server and turns each of its tools into an ordinary
`Tool` value. From there the runner treats them like any other tool:
it validates arguments, executes the calls, appends `ToolRequested`
and `ToolCompleted` events, applies the run's tool-failure policy, and
reports everything to `on_event`. Nothing about the client changes, so
this works with the provider clients and with Claude Code alike.

```baml
class EchoBack {
    reply: string,
}

function EchoThroughMcp() -> EchoBack {
    client: "anthropic/claude-haiku-4-5"
    tools: mcp.tools("everything", "npx", ["-y", "@modelcontextprotocol/server-everything"])
    prompt: `
        Call the echo tool with the message 'hello', then return its
        exact reply.
        ${ctx.output_format}
    `
}

let result: EchoBack = EchoThroughMcp();
```

`tools:` takes an expression and evaluates it once, at spec creation,
so the connection opens when the spec is created. `mcp.tools(...)` is
`McpConnection.connect(...).tools()` with the connection held for the
process; for an explicit lifecycle, connect yourself and close with
`defer`:

```baml
let conn: McpConnection = mcp.McpConnection.connect(
    "everything",
    "npx",
    ["-y", "@modelcontextprotocol/server-everything"],
);
defer {
    conn.close()
}
```

The reference implementation builds the equivalent spec manually with
`Toolbox.new(conn.tools())` until the call desugar exists
(`baml_src/howto/`).

`connect` starts the server as a child process and performs the
protocol handshake. The connection stays open across turns; `close()`
ends it, and `defer` guarantees it ends when the surrounding function
returns. `tools()` asks the server for its catalog and builds one
`Tool` per entry with `ai.raw_tool`. The server already describes each
tool's parameters as JSON Schema, so its schema is used unchanged;
when the model calls the tool, the handler forwards the call over the
connection and returns the server's text response as the result.

A server reply marked as an error throws inside the handler. The
runner appends `ToolFailed`, and the model sees the failure as the
call's result, the same as any tool error
(`../02_guides/01_functions/02_tools.md`).

This works offline in tests. A fake server is any program that
answers the protocol on its standard input and output; the reference
tree's `tests/mcp.baml` uses a short shell script as the server and
drives the loop with `ScriptedClient`, so the whole path runs with no
network and nothing installed.

Compared with attaching servers to Claude Code directly
(`05_attach_mcp_servers_to_claude_code.md`): this page's form records
every call in the journal and works with every client; the Claude
Code form works only there, and its calls happen inside one model
turn, unrecorded. Why both forms exist is recorded in
`../05_appendix/02_alternatives_considered.md`.
