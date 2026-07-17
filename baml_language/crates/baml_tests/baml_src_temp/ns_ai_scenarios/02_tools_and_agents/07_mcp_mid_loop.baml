class AddMcpServerArgs {
  server: string,
}

class McpLookupOrderArgs {
  order_id: string,
}

function add_mcp_server_tool() -> root.ai.Tool throws never {
  root.ai.Tool.typed<AddMcpServerArgs>(
    "add_mcp_server",
    "Connect an MCP server and add its tools on the next agent turn. Available server: orders.",
  )
}

class FakeMcpConnection {
  discoveries: int,
  looked_up_orders: string[],

  function tools(self) -> root.ai.Tool[] throws never {
    self.discoveries = self.discoveries + 1;
    [root.ai.Tool.from_json_schema(
      "mcp__orders__lookup_order",
      "Look up an order through the connected orders MCP server.",
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

// The bootstrap tool changes application state. The prepare-step hook is the
// single place that applies those requests to the driver-owned ToolRegistry,
// so roster changes are versioned and emitted before the next provider step.
class McpServerBroker {
  connection: FakeMcpConnection,
  pending_servers: string[],
  connected_servers: string[],

  function request_server(self, call: root.ai.ToolCall) -> root.ai.ToolResult throws never {
    let args = baml.json.from_json<AddMcpServerArgs>(call.args) catch (e) {
      _ => return root.ai.ToolResult.error(call, "invalid add_mcp_server arguments"),
    };
    if (args.server != "orders") {
      return root.ai.ToolResult.error(call, "unknown MCP server: " + args.server);
    }
    if (self.connected_servers.filter((name: string) -> bool { name == args.server }).length() == 0
        && self.pending_servers.filter((name: string) -> bool { name == args.server }).length() == 0) {
      self.pending_servers.push(args.server);
    }
    root.ai.ToolResult.ok(call, { "server": args.server, "status": "connection_requested" })
  }

  function activate_pending(self, registry: root.ai.ToolRegistry) -> null
      throws baml.errors.ToolError {
    let pending = self.pending_servers.slice(0, self.pending_servers.length());
    self.pending_servers = [];
    //# Activate MCP servers requested by the model
    for (let server in pending) {
      if (server == "orders"
          && self.connected_servers.filter((name: string) -> bool { name == server }).length() == 0) {
        //## Discover and register this server's tools
        for (let tool in self.connection.tools()) {
          registry.add(tool);
        }
        self.connected_servers.push(server);
      }
    }
    null
  }
}

class McpBootstrapHooks {
  broker: McpServerBroker,

  implements root.ai.AgentHooks {
    function prepare_step(self, context: root.ai.StepContext) -> root.ai.StepPlan
        throws baml.errors.ToolError {
      //# Apply bootstrap requests before the next model step
      self.broker.activate_pending(context.tool_registry);
      root.ai.StepPlan { provider: null, tools: null, stop: null }
    }
  }
}

function dispatch_with_mcp_bootstrap(
  broker: McpServerBroker,
  calls: root.ai.ToolCall[],
) -> root.ai.ToolResult[] throws never {
  calls.map((call: root.ai.ToolCall) -> root.ai.ToolResult {
    if (call.name == "add_mcp_server") {
      broker.request_server(call)
    } else if (call.name == "mcp__orders__lookup_order") {
      broker.connection.call(call)
    } else {
      root.ai.ToolResult.error(call, "unknown bootstrap or MCP tool")
    }
  })
}

function McpBootstrap_task(provider: root.ai.Provider) -> root.ai.Task<Resolution> throws never {
  root.ai.task_named<Resolution>(
    provider,
    "McpBootstrap",
    { "order_id": "O-42" },
    [add_mcp_server_tool()],
    default_task_options(),
    (ctx: baml.llm.Context) -> baml.llm.PromptAst {
      baml.llm.assemble_prompt_ast(
        ["", "\n", ""],
        [
          `Find the status of order O-42. You must first call add_mcp_server with server "orders". After that succeeds, call the newly available orders MCP lookup tool. Do not return the final answer until both tool calls have completed.`,
          ctx.output_format,
        ],
      )
    },
  )
}

function mcp_broker() -> McpServerBroker throws never {
  McpServerBroker {
    connection: FakeMcpConnection { discoveries: 0, looked_up_orders: [] },
    pending_servers: [],
    connected_servers: [],
  }
}

test "guide 02.07: a bootstrap tool adds an MCP server between model steps" {
  //# Start with only the MCP bootstrap tool
  let broker = mcp_broker();
  let registry = root.ai.ToolRegistry.new([add_mcp_server_tool()]);
  let provider = root.ai.ScriptedToolProvider {
    turns: [
      root.ai.ToolCalls { calls: [root.ai.ToolCall {
        id: "call-1",
        name: "add_mcp_server",
        args: { "server": "orders" },
      }] },
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
  let options = guide_agent_options(
    [],
    (calls: root.ai.ToolCall[]) -> root.ai.ToolResult[] {
      dispatch_with_mcp_bootstrap(broker, calls)
    },
    tool_registry = registry,
    hooks = McpBootstrapHooks { broker: broker },
  );

  //# Run across bootstrap, discovery, MCP execution, and completion
  let outcome = root.ai.drivers.run_agent<Resolution>(McpBootstrap_task(provider), options);

  //# Verify the model caused discovery rather than a step-number rule
  assert.equal(broker.connected_servers, ["orders"])
  assert.equal(broker.connection.discoveries, 1)
  assert.equal(broker.connection.looked_up_orders, ["O-42"])
  match (outcome) {
    let done: root.ai.Done<Resolution> => match (done.transcript) {
      let transcript: root.ai.ScriptedToolTranscript => {
        assert.equal(transcript.observed_tool_rosters[0], ["add_mcp_server"]);
        assert.equal(
          transcript.observed_tool_rosters[1],
          ["add_mcp_server", "mcp__orders__lookup_order"],
        );
        assert.equal(transcript.submitted_results.length(), 2);
      },
      _ => assert.is_true(false),
    },
    _ => assert.is_true(false),
  }
}

testset "integ-test-guide-02-mcp-bootstrap" {
  test "guide 02.07 live OpenAI: the model adds and uses an MCP server" {
    let broker = mcp_broker();
    let registry = root.ai.ToolRegistry.new([add_mcp_server_tool()]);
    let observer = GuideObserver { kinds: [] };
    let provider = root.ai.OpenAi {
      ...root.ai.openai(),
      model: "gpt-5.6-luna",
      api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
    };
    let outcome = root.ai.drivers.run_agent<Resolution>(
      McpBootstrap_task(provider),
      guide_agent_options(
        [],
        (calls: root.ai.ToolCall[]) -> root.ai.ToolResult[] {
          dispatch_with_mcp_bootstrap(broker, calls)
        },
        tool_registry = registry,
        hooks = McpBootstrapHooks { broker: broker },
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
