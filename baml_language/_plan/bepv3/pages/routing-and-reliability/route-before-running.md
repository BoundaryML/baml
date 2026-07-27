# Route before running

Routing chooses a provider before any provider operation begins.

## Utilities used

| Utility | Purpose |
| --- | --- |
| `task.with_provider(...)` | Returns a rebound task |
| Application router | Chooses from domain or operational data |

## Example

```baml
class Resolution {
  reply: string,
}

function ResolveTicket(message: string) -> Resolution {
  provider: FastModel
  prompt: `
    Resolve this support ticket.

    ${message}

    ${ctx.output_format}
  `
}

function route_ticket(message: string) -> ai.CompletionProvider {
  if (message.to_lower_case().contains("legal")) {
    CarefulModel
  } else {
    FastModel
  }
}

let task = ResolveTicket.task("I need help with a legal notice.");
let routed = task.with_provider(route_ticket("I need help with a legal notice."));
let resolution = routed.run(runner = ai.run.Completion.new())
```

The router is normal application code. It may use tenant policy, data
residency, cost, or request priority.

Its return type keeps the capability required by `Completion`. Returning the
broader `ai.Provider` interface would erase that static proof and require
explicit runtime capability negotiation.

Routing is complete before execution, so it does not need replay policy. A
mid-run provider switch is different because a conversation already exists.

[Back to routing and reliability](../routing-and-reliability.md)
