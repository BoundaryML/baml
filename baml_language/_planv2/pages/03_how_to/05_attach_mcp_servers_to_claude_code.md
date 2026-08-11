# Attach MCP servers to Claude Code

This page gives the Claude Code client access to MCP servers: at
construction, or in the middle of a run.

`ClaudeCodeClient` runs the `claude` CLI as a local process. The CLI
is itself a coding agent with its own tool system, and it can connect
to MCP servers — separate local processes that provide additional
tools over the Model Context Protocol
(`../02_guides/03_clients/05_the_built_in_clients.md`). The client
exposes that ability as configuration: the `mcp_servers` field maps a
server name to that server's launch configuration, in the same shape
Claude Code users write in an `.mcp.json` file.

On every model turn the client reads the map and passes it to the CLI.
`--mcp-config` carries the servers. `--strict-mcp-config` limits the
CLI to exactly these servers; without it the CLI would also load the
user's and the project's own MCP configuration, and the run would
depend on the machine it runs on. `--allowedTools=mcp__<name>`
pre-approves each server's tools, because the CLI runs
non-interactively during a turn and nothing can answer a permission
prompt. Claude Code's built-in tools are a separate setting
(`harness_tools`); the two do not interact.

To attach a server for the whole run, set the field at construction:

```baml
let cfg: map<string, unknown> = {};
let _ = cfg.set("command", "npx");
let _ = cfg.set("args", ["-y", "@modelcontextprotocol/server-everything"]);
let servers: map<string, json> = {};
let _ = servers.set("everything", baml.json.to_json(cfg));
let c: Client = ClaudeCodeClient.new(mcp_servers = servers);
```

An attached server's tools belong to the CLI, not to the function's
toolbox. The model uses them inside a turn the same way it uses Claude
Code's own tools: the runner never executes them, and no journal event
records them — the journal shows only the turn's result. The client's
log output does show the inner calls as they happen. If an attached
server's tools change anything outside the run — write files, send
requests to other systems — a failed turn is no longer safe to retry,
because the failure may have arrived after such a change
(`../02_guides/03_clients/04_reliability.md`).

To attach a server mid-run, add an entry to the map between turns.
The client reads the map fresh on every turn, and every turn starts a
new CLI process, so a new entry takes effect on the turn after it is
added. A BAML tool can therefore let the model attach servers itself:

```baml
function attach_mcp_tool(c: ClaudeCodeClient) -> Tool {
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
```

The closure captures the client value, so the tool call updates the
same client the run is using. The function declares the tool and the
brief; the run is an ordinary call:

```baml
class EchoBack {
    reply: string,
}

function DynamicMcpEcho(server_pkg: string) -> EchoBack {
    client: "claude-code/claude-haiku-4-5"
    tools: [attach_mcp]
    prompt: `
        You have no echo capability yet. First call attach_mcp with
        name "everything", command "npx", args ["-y", "${server_pkg}"].
        After it reports attached, an mcp__everything__echo tool will be
        installed in your environment on your next turn; call it with the
        message 'hello' and return its exact reply.
        ${ctx.output_format}
    `
}

let result: EchoBack = DynamicMcpEcho("@modelcontextprotocol/server-everything");
```

`attach_mcp` needs to update the client the run is using; the
reference implementation binds the two by building this spec manually
with the tool closing over the client value
(`baml_src/howto/attach_mcp.baml`), pending the call desugar and an
ambient way for a tool to reach the run's client.

The attachment itself is an ordinary tool call, so `ToolRequested` and
`ToolCompleted` events record that it happened. The attached server's
later calls are not recorded, as above. The tool's description tells
the model that the new tools arrive on its next turn; the turn that
requested them cannot use them.

The application cannot attach a server while `run()` is executing.
Input into a running loop is steering, which arrives with sessions
(`../05_appendix/03_future_phases.md`).

For MCP tools that are recorded in the journal and work with every
client, not only Claude Code, use the `root.mcp` library instead
(`06_use_mcp_tools_with_any_client.md`). Why both forms exist is
recorded in `../05_appendix/02_alternatives_considered.md`.
