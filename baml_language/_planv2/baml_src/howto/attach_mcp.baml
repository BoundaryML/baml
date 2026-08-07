// How-to: attach MCP servers to the Claude Code client, statically or
// mid-run through a BAML tool. Each invoke reads the client's mcp_servers
// map when it renders the CLI arguments, so a server added between turns
// attaches on the next turn's process.
// Mirrors _planv2/pages/03_how_to/05_attach_mcp_servers_to_claude_code.md.

// A tool that attaches an MCP server to the given client. The closure
// captures the client value; the mutation is visible to the next invoke
// because objects are reference values and rendering reads the map fresh.
// Built with raw_tool: the reflect boundary does not yet decode a JSON
// string array into a string[] parameter, and a raw handler reads the
// argument map directly.
function attach_mcp_tool(c: root.claude_code.ClaudeCodeClient) -> root.ai.Tool {
    let str_schema: map<string, unknown> = {};
    let _ = str_schema.set("type", "string");
    let arr_schema: map<string, unknown> = {};
    let _ = arr_schema.set("type", "array");
    let _ = arr_schema.set("items", str_schema);
    let props: map<string, unknown> = {};
    let _ = props.set("name", str_schema);
    let _ = props.set("command", str_schema);
    let _ = props.set("args", arr_schema);
    let schema: map<string, unknown> = {};
    let _ = schema.set("type", "object");
    let _ = schema.set("properties", props);
    let _ = schema.set("required", ["name", "command", "args"]);
    let _ = schema.set("additionalProperties", false);
    root.ai.raw_tool(
        "attach_mcp",
        "Attach an MCP server (a local command) to the harness. Its tools become available on the NEXT turn, not this one.",
        schema,
        (raw: map<string, unknown>) -> {
            let j = baml.json.to_json(raw);
            let server = baml.json.path_or<string>(j, ".name", "");
            let cfg: map<string, unknown> = {};
            let _ = cfg.set("command", baml.json.path_or<string>(j, ".command", ""));
            let _ = cfg.set("args", baml.json.path_or<string[]>(j, ".args", []));
            let _ = c.mcp_servers.set(server, baml.json.to_json(cfg));
            `attached MCP server ${server}; its tools are available from the next turn`
        },
    )
}

test "a fresh client renders no mcp config" {
    let c = root.claude_code.ClaudeCodeClient.new();
    assert.is_true(root.claude_code.internal.mcp_config_json(c) == null)
}

test "attach_mcp mutates the client and the next render sees the server" {
    let c = root.claude_code.ClaudeCodeClient.new();
    // the runner would dispatch this through Tool.call on the model's args
    let call_args: map<string, unknown> = {};
    let _ = call_args.set("name", "filesystem");
    let _ = call_args.set("command", "npx");
    let _ = call_args.set("args", ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]);
    let out = attach_mcp_tool(c).call(call_args);
    assert.contains(out, "next turn");
    let rendered = root.claude_code.internal.mcp_config_json(c) ?? "";
    assert.contains(rendered, "mcpServers");
    assert.contains(rendered, "filesystem");
    assert.contains(rendered, "server-filesystem");
    // the server's tools are allowed, or -p mode would deny every call
    assert.equal(root.claude_code.internal.allowed_mcp_tools(c), "mcp__filesystem")
}

test "static attachment via the constructor renders the same config" {
    let cfg: map<string, unknown> = {};
    let _ = cfg.set("command", "my-mcp");
    let _ = cfg.set("args", ["--stdio"]);
    let servers: map<string, json> = {};
    let _ = servers.set("search", baml.json.to_json(cfg));
    let c = root.claude_code.ClaudeCodeClient.new(mcp_servers = servers);
    let rendered = root.claude_code.internal.mcp_config_json(c) ?? "";
    assert.contains(rendered, "search");
    assert.contains(rendered, "my-mcp")
}
