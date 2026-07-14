# Connect an MCP server halfway through a run

MCP is a runtime tool source. The connection supplies schemas and dispatch;
the agent driver still owns the active roster and loop.

## Start with discovery only

```baml
let registry = ai.ToolRegistry.new([tool_search])
```

## Connect when the run needs more capability

```baml
class McpDiscoveryHooks {
  // ...connection policy and retained MCP resources...

  implements ai.AgentHooks {
    function prepare_step(self, ctx: ai.StepContext) -> ai.StepPlan throws never {
      if (ctx.step == 2 && !ctx.tool_registry.contains("search_policy")) {
        let server = baml.mcp.connect(ctx.state.get_or_panic("policy_mcp_url"))
        ctx.state.set("policy_mcp", server)
        ctx.tool_registry.add_all(server.tools())
      }

      ai.StepPlan {
        provider: null,
        tools: ctx.tool_registry.snapshot(),
        stop: null,
      }
    }

    // ...other AgentHooks methods use their defaults...
  }
}
```

The subsequent provider request includes `search_policy`. When the model calls
it, registry dispatch routes the call through the same MCP connection.

## Lifecycle

The application or driver must retain and close the MCP connection. Adding its
schemas without retaining its dispatcher creates tools that can be advertised
but never executed.

## Security

Treat newly discovered schemas as untrusted input. Apply name-collision,
permission, argument-validation, and result-redaction policy before activation.

## Related design and scenarios

- [Adding MCP halfway through](../../pages/05-tools-and-agents.md#adding-mcp-halfway-through)
- Scenarios 13 searchable tools, 39 harness extensibility
