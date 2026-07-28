class AddMcpServerArgs {
  server: string,
}

class McpLookupOrderArgs {
  order_id: string,
}

function add_mcp_server_tool(broker: McpServerBroker) -> root.ai.Tool throws never {
  root.ai.tool(
    broker.request_server,
    name = "add_mcp_server",
  )
}

class FakeMcpConnection {
  discoveries: int,
  looked_up_orders: string[],

  function tools(self) -> root.ai.Tool[] throws never {
    self.discoveries = self.discoveries + 1;
    [root.ai.tool_from_json_schema(
      "mcp__orders__lookup_order",
      "Look up an order through the connected orders MCP server.",
      {
        "type": "object",
        "properties": { "order_id": { "type": "string" } },
        "required": ["order_id"],
      },
      self.lookup_order,
    )]
  }

  function lookup_order(self, order_id: string) -> json throws never {
    self.looked_up_orders.push(order_id);
    { "order_id": order_id, "status": "shipped" }
  }
}

// The bootstrap tool changes application state. The prepare-step callback turns
// pending requests into a complete next roster, which the runner validates and
// applies before the next provider step.
class McpServerBroker {
  connection: FakeMcpConnection,
  pending_servers: string[],
  connected_servers: string[],

  /// Connect an MCP server and add its tools on the next agent turn. Available server: orders.
  function request_server(self, server: string) -> json throws never {
    if (server != "orders") {
      return { "server": server, "status": "unknown_server" };
    }
    if (self.connected_servers.filter((name: string) -> bool { name == server }).length() == 0
        && self.pending_servers.filter((name: string) -> bool { name == server }).length() == 0) {
      self.pending_servers.push(server);
    }
    { "server": server, "status": "connection_requested" }
  }

  function activate_pending(self, current_tools: root.ai.Tool[]) -> root.ai.Tool[]? throws never {
    let pending = self.pending_servers.slice(0, self.pending_servers.length());
    self.pending_servers = [];
    let next_tools = current_tools.slice(0, current_tools.length());
    let changed = false;
    //# Activate MCP servers requested by the model
    for (let server in pending) {
      if (server == "orders"
          && self.connected_servers.filter((name: string) -> bool { name == server }).length() == 0) {
        //## Discover and add this server's tools to the next roster
        for (let tool in self.connection.tools()) {
          next_tools.push(tool);
          changed = true;
        }
        self.connected_servers.push(server);
      }
    }
    if (changed) { next_tools } else { null }
  }
}

function McpBootstrap_task(
  provider: root.ai.Provider,
  bootstrap_tool: root.ai.Tool,
) -> root.ai.Task<Resolution> throws never {
  root.ai.task_named<Resolution>(
    provider,
    "McpBootstrap",
    { "order_id": "O-42" },
    [bootstrap_tool],
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
