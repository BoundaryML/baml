// Live smoke: dynamic harness-native MCP. The model starts with no echo
// capability, attaches the server itself through the attach_mcp tool, and
// uses the server's echo tool natively inside the next turn's process.
//   ../target/debug/baml-cli run -e 'live_claude_code_dynamic_mcp()'

function dynamic_mcp_spec(c: claude_code.ClaudeCodeClient) -> ai.FunctionSpec<EchoBack> {
    let args: map<string, unknown> = {};
    ai.FunctionSpec {
        spec_name: "DynamicMcpEcho",
        args: args,
        prompt_template: ai.Prompt {
            template: (output_format: string) -> {
                `
                    You have no echo capability yet. First call attach_mcp with
                    name "everything", command "npx",
                    args ["-y", "@modelcontextprotocol/server-everything"].
                    After it reports attached, an mcp__everything__echo tool will be
                    installed in your environment on your next turn; call it with the
                    message 'baml-dynamic-ok' and return its exact reply.
                    ${output_format}
                `
            },
        },
        toolbox: ai.Toolbox.new([attach_mcp_tool(c)]),
        default_client: () -> {
            c
        },
    }
}

function live_claude_code_dynamic_mcp() -> string {
    let c = claude_code.ClaudeCodeClient.new();
    let result = ai.Agent<EchoBack>.new(on_event = log_event).run(dynamic_mcp_spec(c));
    let attached = claude_code.internal.mcp_config_json(c) ?? "";
    `reply=${result.value.reply} servers_attached=${attached.includes("everything")}`
}
