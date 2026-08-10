// How-to: attach MCP servers to the Claude Code client, statically or
// mid-run through a BAML tool. Each invoke reads the client's mcp_servers
// map when it renders the CLI arguments, so a server added between turns
// attaches on the next turn's process.
// Mirrors _planv2/pages/03_how_to/05_attach_mcp_servers_to_claude_code.md.

// A tool that attaches an MCP server to the given client. The closure
// captures the client value; the mutation is visible to the next invoke
// because objects are reference values and rendering reads the map fresh.
// An ordinary signature-derived tool: the schema comes from the closure's
// parameters, and the boundary decode in reflect.call_any (B-1174) binds
// the model's JSON string array into the string[] parameter.
function attach_mcp_tool(c: claude_code.ClaudeCodeClient) -> ai.tools.Tool {
    ai.tools.tool(
        (name: string, command: string, args: string[]) -> string {
            let cfg: map<string, unknown> = {};
            let _ = cfg.set("command", command);
            let _ = cfg.set("args", args);
            let _ = c.mcp_servers.set(name, baml.json.to_json(cfg));
            `attached MCP server ${name}; its tools are available from the next turn`
        },
        name = "attach_mcp",
        description = "Attach an MCP server (a local command) to the harness. Its tools become available on the NEXT turn, not this one.",
    )
}

test "a fresh client renders no mcp config" {
    let c = claude_code.ClaudeCodeClient.new();
    assert.is_true(claude_code.internal.mcp_config_json(c) == null)
}

test "attach_mcp mutates the client and the next render sees the server" {
    let c = claude_code.ClaudeCodeClient.new();
    // the runner would dispatch this through Tool.call on the model's args
    let call_args: map<string, unknown> = {};
    let _ = call_args.set("name", "filesystem");
    let _ = call_args.set("command", "npx");
    let _ = call_args.set("args", ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]);
    let out = attach_mcp_tool(c).call(call_args);
    assert.contains(out, "next turn");
    let rendered = claude_code.internal.mcp_config_json(c) ?? "";
    assert.contains(rendered, "mcpServers");
    assert.contains(rendered, "filesystem");
    assert.contains(rendered, "server-filesystem");
    // the server's tools are allowed, or -p mode would deny every call
    assert.equal(claude_code.internal.allowed_mcp_tools(c), "mcp__filesystem")
}

test "static attachment via the constructor renders the same config" {
    let cfg: map<string, unknown> = {};
    let _ = cfg.set("command", "my-mcp");
    let _ = cfg.set("args", ["--stdio"]);
    let servers: map<string, json> = {};
    let _ = servers.set("search", baml.json.to_json(cfg));
    let c = claude_code.ClaudeCodeClient.new(mcp_servers = servers);
    let rendered = claude_code.internal.mcp_config_json(c) ?? "";
    assert.contains(rendered, "search");
    assert.contains(rendered, "my-mcp")
}
