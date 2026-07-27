# Observe an Agent

An observer receives model, tool, provider, usage, and terminal events without
changing execution.

## Utilities used

| Utility | Purpose |
| --- | --- |
| `ai.AgentObserver` | Read-only event callback |
| `ai.AgentEvent` | Typed event union |
| `observers` | Agent runner configuration |

## Example

```baml
class Resolution {
  reply: string,
}

function lookup_order(order_id: string) -> string {
  "out for delivery"
}

function ResolveTicket(message: string) -> Resolution {
  provider: "openai/gpt-5.6-luna"
  prompt: `
    Resolve this ticket.

    ${message}

    ${ctx.output_format}
  `
  tools: [lookup_order]
}

class ConsoleObserver {
  implements ai.AgentObserver {
    function on_event(self, event: ai.AgentEvent) -> null {
      log.info(event)
    }
  }
}

let outcome = ResolveTicket.task("Where is order-42?").run(
  runner = ai.run.Agent.new(
    observers = [ConsoleObserver {}],
  ),
)
```

Observers cannot replace tools, change providers, or block calls. Those are
hook decisions. An observer may write logs, update a UI, export traces, or
aggregate metrics.

[Back to observability and testing](../observability-and-testing.md)
