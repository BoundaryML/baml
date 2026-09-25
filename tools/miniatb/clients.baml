client Claude = claude_code.ClaudeCodeClient.new(
    model = "sonnet",
    executable = "/app/deployment/claude",
    harness_tools = [],
    timeout_ms = 600000,
    capture_wire = false,
);

client Jev = typesafeai.Client.new(capture_wire = false);
