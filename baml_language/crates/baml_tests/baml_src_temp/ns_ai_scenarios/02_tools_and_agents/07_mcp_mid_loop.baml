class McpLookupOrderArgs {
  order_id: string,
}

class FakeMcpConnection {
  discoveries: int,
  looked_up_orders: string[],

  function tools(self) -> root.ai.Tool[] throws never {
    self.discoveries = self.discoveries + 1;
    [root.ai.Tool.from_json_schema(
      "mcp__orders__lookup_order",
      "Look up an order discovered from MCP.",
      {
        "type": "object",
        "properties": { "order_id": { "type": "string" } },
        "required": ["order_id"],
      },
    )]
  }

  function call(self, call: root.ai.ToolCall) -> root.ai.ToolResult throws never {
    let args = baml.json.from_json<McpLookupOrderArgs>(call.args) catch (e) {
      _ => return root.ai.ToolResult.error(call, "invalid MCP lookup_order arguments"),
    };
    self.looked_up_orders.push(args.order_id);
    root.ai.ToolResult.ok(call, { "order_id": args.order_id, "status": "shipped" })
  }
}

class McpDiscoveryHooks {
  connection: FakeMcpConnection,

  implements root.ai.AgentHooks {
    function prepare_step(self, context: root.ai.StepContext) -> root.ai.StepPlan
        throws baml.errors.ToolError {
      //# Discover MCP tools after the first model step
      if (context.step == 1) {
        //## Register every discovered MCP tool
        for (let tool in self.connection.tools()) {
          context.tool_registry.add(tool);
        }
      }
      root.ai.StepPlan { provider: null, tools: null, stop: null }
    }
  }
}

function dispatch_with_mcp(
  connection: FakeMcpConnection,
  calls: root.ai.ToolCall[],
) -> root.ai.ToolResult[] throws never {
  calls.map((call: root.ai.ToolCall) -> root.ai.ToolResult {
    if (call.name == "mcp__orders__lookup_order") {
      connection.call(call)
    } else {
      guide_dispatch([call])[0]
    }
  })
}

test "guide 02.07: MCP discovery changes the roster between model steps" {
  //# Configure MCP discovery and the initial registry
  let connection = FakeMcpConnection { discoveries: 0, looked_up_orders: [] };
  let registry = root.ai.ToolRegistry.new([knowledge_tool()]);
  let provider = root.ai.ScriptedToolProvider {
    turns: [
      root.ai.ToolCalls { calls: [one_knowledge_call()] },
      root.ai.ToolCalls { calls: [root.ai.ToolCall {
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
  //# Attach mid-loop discovery and MCP dispatch
  let options = guide_agent_options(
    [],
    (calls: root.ai.ToolCall[]) -> root.ai.ToolResult[] {
      dispatch_with_mcp(connection, calls)
    },
    tool_registry = registry,
    hooks = McpDiscoveryHooks { connection: connection },
  );

  //# Run the agent across changing tool rosters
  let outcome = root.ai.drivers.run_agent<Resolution>(
    ResolveTicketWithTools_task(sample_ticket(), provider, []),
    options,
  );

  //# Verify discovery and MCP execution
  assert.equal(connection.discoveries, 1)
  assert.equal(connection.looked_up_orders, ["O-42"])
  //# Verify each model step saw the expected roster
  match (outcome) {
    let done: root.ai.Done<Resolution> => match (done.transcript) {
      let transcript: root.ai.ScriptedToolTranscript => {
        assert.equal(transcript.observed_tool_rosters[0], ["search_knowledge"]);
        assert.equal(
          transcript.observed_tool_rosters[1],
          ["search_knowledge", "mcp__orders__lookup_order"],
        );
        assert.equal(transcript.submitted_results.length(), 2);
        assert.is_true(!transcript.submitted_results[1][0].is_error);
      },
      _ => assert.is_true(false),
    },
    _ => assert.is_true(false),
  }
}
