// A harness client: the authenticated Claude Code CLI as a root.ai.Client.
// The transport is a local process, not HTTP; the output contract travels
// natively via --json-schema; BAML tools ride the `outcome` envelope.

class ClaudeCodeClient {
    executable: string,
    model: string,
    cwd: string?,
    timeout_ms: int,
    permission_mode: string,
    harness_tools: string[], // Claude Code's OWN tools; BAML tools use the envelope

    function new(
        model: string = "haiku",
        executable: string = "claude",
        cwd: string? = null,
        timeout_ms: int = 240000,
        permission_mode: string = "default",
        harness_tools: string[] = [],
    ) -> ClaudeCodeClient {
        ClaudeCodeClient {
            executable: executable,
            model: model,
            cwd: cwd,
            timeout_ms: timeout_ms,
            permission_mode: permission_mode,
            harness_tools: harness_tools,
        }
    }

    implements root.ai.Client {
        function id(self) -> string {
            `claude-code/${self.model}`
        }

        function invoke(self, input: root.ai.ModelTurnInput) -> root.ai.ModelTurn {
            root.claude_code.internal.invoke(self, input)
        }
    }
}
