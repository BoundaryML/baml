test "guide 02.07: a bootstrap tool adds an MCP server between model steps" {
  //# Start with only the MCP bootstrap tool
  let broker = mcp_broker();
  let bootstrap_tool = add_mcp_server_tool(broker);
  let registry = root.ai.tools.ToolRegistry.new([bootstrap_tool]);
  let provider = root.ai.testing.ScriptedToolProvider {
    turns: [
      root.ai.tools.ToolCalls { calls: [root.ai.tools.ToolCall {
        id: "call-1",
        name: "add_mcp_server",
        args: { "server": "orders" },
      }] },
      root.ai.tools.ToolCalls { calls: [root.ai.tools.ToolCall {
        id: "call-2",
        name: "mcp__orders__lookup_order",
        args: { "order_id": "O-42" },
      }] },
    ],
    final_output: `{
      "category":"billing",
      "priority":"Urgent",
      "summary":"Order checked through MCP",
      "reply":"Order O-42 has shipped."
    }`,
    usage_per_step: null,
  };
  let runner = root.ai.run.Agent<Resolution>.new(
    tools = [],
    tool_registry = registry,
    prepare_step = (context) -> {
      //# Apply bootstrap requests before the next model step
      let next_tools = broker.activate_pending(context.tools);
      root.ai.tools.StepPlan { provider: null, tools: next_tools, stop: null }
    },
  );

  //# Run across bootstrap, discovery, MCP execution, and completion
  let task = McpBootstrap_task(provider, bootstrap_tool);
  let outcome = task.run(runner = runner);

  //# Verify the model caused discovery rather than a step-number rule
  assert.equal(broker.connected_servers, ["orders"])
  assert.equal(broker.connection.discoveries, 1)
  assert.equal(broker.connection.looked_up_orders, ["O-42"])
  match (outcome) {
    let done: root.ai.Done<Resolution> => match (done.conversation) {
      let conversation: root.ai.testing.ScriptedToolConversation => {
        assert.equal(conversation.observed_tool_rosters[0], ["add_mcp_server"]);
        assert.equal(
          conversation.observed_tool_rosters[1],
          ["add_mcp_server", "mcp__orders__lookup_order"],
        );
        assert.equal(conversation.submitted_results.length(), 2);
      },
      _ => assert.is_true(false),
    },
    _ => assert.is_true(false),
  }
}

testset "integ-test-guide-02-mcp-bootstrap" {
  test "guide 02.07 live OpenAI: the model adds and uses an MCP server" {
    let broker = mcp_broker();
    let bootstrap_tool = add_mcp_server_tool(broker);
    let registry = root.ai.tools.ToolRegistry.new([bootstrap_tool]);
    let observer = GuideObserver { kinds: [] };
    let provider = root.openai.OpenAIProvider {
      ...root.openai.responses(),
      model: "gpt-5.6-luna",
      api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
    };
    let task = McpBootstrap_task(provider, bootstrap_tool);
    let outcome = task.run(
      runner = root.ai.run.Agent<Resolution>.new(
        tools = [],
        tool_registry = registry,
        prepare_step = (context) -> {
          //# Apply bootstrap requests before the next model step
          let next_tools = broker.activate_pending(context.tools);
          root.ai.tools.StepPlan { provider: null, tools: next_tools, stop: null }
        },
        observers = [observer],
      ),
    );
    log.info("live MCP bootstrap " + {
      "connected_servers": broker.connected_servers,
      "looked_up_orders": broker.connection.looked_up_orders,
      "events": observer.kinds,
    }.to_string());
    assert.equal(broker.connected_servers, ["orders"])
    assert.equal(broker.connection.looked_up_orders, ["O-42"])
    assert.contains(observer.kinds.join(","), "tool_roster_changed")
    match (outcome) {
      let done: root.ai.Done<Resolution> => assert.contains(done.value.reply, "O-42"),
      _ => assert.is_true(false),
    }
  }
}
