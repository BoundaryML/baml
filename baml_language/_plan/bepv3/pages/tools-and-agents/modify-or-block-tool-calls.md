# Modify or block a tool call

A before-tool hook may narrow arguments, replace a call, or block it.

## Utilities used

| Utility | Purpose |
| --- | --- |
| `ai.BeforeToolCall` | Proposed call and run context |
| `ai.ToolDecision.replace` | Uses a rewritten call |
| `ai.ToolDecision.block` | Produces a correlated denial |

## Example

```baml
class Resolution {
  reply: string,
}

function lookup_order(customer_id: string, order_id: string) -> string {
  `order ${order_id} belongs to ${customer_id}`
}

function ResolveTicket(customer_id: string, message: string) -> Resolution {
  provider: "openai/gpt-5.6-luna"
  prompt: `
    Resolve this ticket for customer ${customer_id}.

    ${message}

    ${ctx.output_format}
  `
  tools: [lookup_order]
}

class CustomerScope {
  customer_id: string,

  implements ai.AgentHooks {
    function before_tool_call(
      self,
      event: ai.BeforeToolCall,
    ) -> ai.ToolDecision {
      if (event.call.name == "lookup_order") {
        ai.ToolDecision.replace(
          ai.ToolCall {
            id: event.call.id,
            name: event.call.name,
            args: {
              ...event.call.args,
              "customer_id": self.customer_id,
            },
          },
        )
      } else {
        ai.ToolDecision.allow(event.call)
      }
    }
  }
}

let outcome = ResolveTicket.task("customer-7", "Where is order-42?").run(
  runner = ai.run.Agent.new(
    hooks = CustomerScope { customer_id: "customer-7" },
  ),
)
```

The replacement keeps the provider call ID. The application remains the
authority for tenant and customer scope even if the model supplied another
value.

[Back to tools and agents](../tools-and-agents.md)
