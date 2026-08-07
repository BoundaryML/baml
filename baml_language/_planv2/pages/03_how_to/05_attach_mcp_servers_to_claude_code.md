# Attach MCP servers to Claude Code

The Claude Code harness has its own MCP support, and `ClaudeCodeClient`
exposes it as configuration. The `mcp_servers` field maps a server name
to its config, in the value shape of an `.mcp.json` `mcpServers` entry.
Every invoke renders the map into the CLI flags: `--mcp-config` carries
the servers, `--strict-mcp-config` keeps the set exact (never inherited
user or project configuration), and `--allowedTools=mcp__<name>` allows
each server's tools, because `-p` mode has nobody to answer a
permission prompt. `--tools` governs only the harness's built-in set,
so `harness_tools` and MCP attachment are independent settings.

Attach statically at construction:

```baml
let cfg: map<string, unknown> = {};
let _ = cfg.set("command", "npx");
let _ = cfg.set("args", ["-y", "@modelcontextprotocol/server-everything"]);
let servers: map<string, json> = {};
let _ = servers.set("everything", baml.json.to_json(cfg));
let c: Client = ClaudeCodeClient.new(mcp_servers = servers);
```

An attached server's tools run inside the harness, within the turn,
like `harness_tools`: the runner does not execute them and the journal
records only the normalized turn. The harness's event stream shows the
inner calls as log lines. With a side-effectful server attached, a
mid-run transport failure cannot claim `Safe`
(`../02_guides/03_clients/05_the_built_in_clients.md`).

To attach a server mid-run, mutate the map between turns. Rendering
reads the map fresh on every invoke and each invoke is a new CLI
process, so a tool that adds an entry attaches the server on the next
turn:

```baml
function attach_mcp_tool(c: ClaudeCodeClient) -> Tool {
    ai.raw_tool(
        "attach_mcp",
        "Attach an MCP server (a local command) to the harness. Its tools become available on the NEXT turn, not this one.",
        attach_mcp_schema(),      // {name, command, args: string[]}
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
```

```baml
let c = ClaudeCodeClient.new();
let spec: FunctionSpec<EchoBack> = ...; // toolbox: [attach_mcp_tool(c)], default_client: c
let result: RunResult<EchoBack> = ai.Agent<EchoBack>.new().run(spec);
```

The closure captures the client value, and objects are reference
values, so the mutation is visible to the next render. The attachment
decision is a real tool call — journaled as `ToolRequested` and
`ToolCompleted` — even though the attached server's own calls are not.
The tool's description tells the model the added tools arrive on the
next turn; the current process cannot gain them.

Mid-run attachment by the application, rather than by the model, is
steering, which this BEP defers to sessions
(`../05_appendix/03_future_phases.md`).

For MCP tools that are journaled and work with every client, use the
`root.mcp` library instead (`06_use_mcp_tools_with_any_client.md`);
`../05_appendix/02_alternatives_considered.md` records how the two
forms relate.
