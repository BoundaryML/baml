# Connect an MCP server halfway through a run

> **Status:** Implemented in the executable reference.

MCP is a runtime tool source. The connection supplies schemas and dispatch;
the agent driver still owns the active roster and loop.

## Start with one bootstrap tool

```baml
let registry = ai.ToolRegistry.new([add_mcp_server])
```

`add_mcp_server` is an ordinary application tool. Its handler validates the
requested server and records a pending connection request. The model therefore
chooses when it needs another capability; the hook does not guess from a step
number.

## Activate the requested server on the next turn

```baml
class McpBootstrapHooks {
  broker: McpServerBroker,

  implements ai.AgentHooks {
    function prepare_step(self, ctx: ai.StepContext) -> ai.StepPlan
        throws baml.errors.ToolError {
      let next_tools = self.broker.activate_pending(ctx.tools)
      ai.StepPlan {
        provider: null,
        tools: next_tools,
        stop: null,
      }
    }
  }
}
```

The other hook methods use their interface defaults. `McpServerBroker` is the
application-owned object that validates requests, retains open connections,
and returns the complete next roster when it discovers tools. It returns
`null` when nothing changed.

The resulting turn sequence is explicit:

```text
turn 1 tools: add_mcp_server
model:        add_mcp_server({ server: "orders" })
application:  validates and records the request
hook:         connects, discovers, and returns the complete next roster
turn 2 tools: add_mcp_server, mcp__orders__lookup_order
model:        mcp__orders__lookup_order({ order_id: "O-42" })
```

The driver validates the returned roster, applies it to its `ToolRegistry`, and
emits `ToolRosterChanged` before the next provider request. When the model calls
the discovered tool, dispatch routes it through the retained MCP connection.

## Lifecycle

The application or driver must retain and close the MCP connection. Adding its
schemas without retaining its dispatcher creates tools that can be advertised
but never executed.

## Security

Treat newly discovered schemas as untrusted input. Apply name-collision,
permission, argument-validation, and result-redaction policy before activation.
The bootstrap handler should expose an allowlist or approval policy rather than
accepting arbitrary URLs supplied by the model.

## Related design


- [Adding MCP halfway through](../specification/05-tools-and-agents.md#adding-mcp-halfway-through)
