// A harness client: the authenticated Claude Code CLI as a ai.Client.
// The transport is a local process, not HTTP; the output contract travels
// natively via --json-schema; BAML tools ride the `outcome` envelope.

class ClaudeCodeClient {
    executable: string,
    model: string,
    cwd: string?,
    timeout_ms: int,
    permission_mode: string,
    harness_tools: string[], // Claude Code's OWN tools; BAML tools use the envelope
    // MCP servers attached to the harness, name -> config (the value shape of
    // an .mcp.json "mcpServers" entry). The map is read fresh on every
    // invoke, so an entry added between turns attaches on the next turn.
    mcp_servers: map<string, json>,

    function new(
        model: string = "haiku",
        executable: string = "claude",
        cwd: string? = null,
        timeout_ms: int = 240000,
        permission_mode: string = "default",
        harness_tools: string[] = [],
        mcp_servers: map<string, json>? = null,
    ) -> ClaudeCodeClient {
        let servers: map<string, json> = {};
        ClaudeCodeClient {
            executable: executable,
            model: model,
            cwd: cwd,
            timeout_ms: timeout_ms,
            permission_mode: permission_mode,
            harness_tools: harness_tools,
            mcp_servers: mcp_servers ?? servers,
        }
    }

    implements ai.Client {
        function id(self) -> string {
            `claude-code/${self.model}`
        }

        // Claude Code is a process-backed client rather than an HTTP client;
        // there is no wire Request to preview. Keep this explicit so adding
        // render to ai.Client cannot accidentally invoke the CLI.
        function render(self, input: ai.ModelTurnInput) -> baml.http.Request {
            throw ai.errors.PreviewUnsupported {
                provider: "claude-code",
                detail: "claude-code has no HTTP request representation",
            }
        }

        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
            root.internal.invoke(self, input) catch_all (e) {
                _ => throw ai.errors.normalize_invoke(e),
            }
        }
    }
}
