// Discover and use a tool from a newly connected MCP server while an agent is
// already running.
function mcp_mid_loop() -> null {
    let broker = mcp_broker();
    let bootstrap_tool = add_mcp_server_tool(broker);
    let registry = ai.tools.ToolRegistry.new([bootstrap_tool]);
    let provider = ai.testing.ScriptedToolProvider {
        turns: [
            ai.tools.ToolCalls {
                calls: [
                    ai.tools.ToolCall {
                        id: "call-1",
                        name: "add_mcp_server",
                        args: { "server": "orders" },
                    },
                ],
            },
            ai.tools.ToolCalls {
                calls: [
                    ai.tools.ToolCall {
                        id: "call-2",
                        name: "mcp__orders__lookup_order",
                        args: { "order_id": "O-42" },
                    },
                ],
            },
        ],
        final_output: `
            {
                        "category":"billing",
                        "priority":"Urgent",
                        "summary":"Order checked through MCP",
                        "reply":"Order O-42 has shipped."
                    }
        `,
        usage_per_step: null,
    };
    let runner = ai.run.Agent<Resolution>.new(tool_registry = registry, prepare_step = (context) -> {
        let next_tools = broker.activate_pending(context.tools);
        ai.tools.StepPlan { provider: null, tools: next_tools, stop: null }
    });
    let outcome = McpBootstrap@task("O-42").with_provider(provider).with_tools([bootstrap_tool]).run(runner = runner);
    log.info(
        {
            "outcome": outcome,
            "servers": broker.connected_servers,
            "orders": broker.connection.looked_up_orders,
        },
    );
    null
}
