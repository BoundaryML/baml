# Discover MCP tools during a run

An MCP connection can produce ordinary executable tools whose handlers capture
that connection.

## Utilities used

| Utility | Purpose |
| --- | --- |
| `ai.mcp.connect` | Opens an MCP connection |
| `connection.list_tools()` | Discovers remote tool definitions |
| `ai.tool_from_json_schema` | Creates an executable dynamic tool |
| `ai.ToolRegistry` | Adds discovered tools for the next step |

## Example

```baml
class Resolution {
  reply: string,
}

class McpDiscovery {
  registry: ai.ToolRegistry,
  connections: ai.mcp.Connection[],

  function add_server(
    self,
    server_url: string,
  ) -> string {
    let connection = ai.mcp.connect(server_url);
    self.connections.push(connection);
    let definitions = connection.list_tools();

    for (let definition in definitions) {
      self.registry.add(
        ai.tool_from_json_schema(
          definition.name,
          definition.description,
          definition.input_schema,
          (args) -> {
            connection.call(definition.name, args)
          },
        ),
      )
    }

    `added ${definitions.length()} tools`
  }

  function close(self) {
    for (let connection in self.connections) {
      connection.close()
    }
  }

  function cleanup(self) {
    self.close()
  }
}

function ResolveTicket(message: string) -> Resolution {
  provider: "openai/gpt-5.6-luna"
  prompt: `
    Resolve this ticket. Add the support MCP server when you need its tools.

    ${message}

    ${ctx.output_format}
  `
}

let registry = ai.ToolRegistry.new([]);
let discovery = McpDiscovery {
  registry: registry,
  connections: [],
};
registry.add(discovery.add_server);
defer { discovery.close() }

let outcome = ResolveTicket.task("Find the account policy for customer-7.").run(
  runner = ai.run.Agent.new(
    tool_registry = registry,
  ),
)
```

There is no MCP-specific dispatch switch. Each generated tool retains a
callable handler bound to the connection.

The discovery object owns every connection. Its deferred `close()` runs after
the Agent finishes; `cleanup()` provides the garbage-collection fallback.
Generated handlers therefore never point at a connection closed at the end of
the bootstrap tool call. Newly added tools are offered on the next provider
step.

[Back to tools and agents](../tools-and-agents.md)
