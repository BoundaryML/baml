# Handoffs and limits

An Agent may return control before producing `T`.

## Utilities used

| Utility | Purpose |
| --- | --- |
| `ai.tool(...).as_handoff()` | Marks a terminal application handoff |
| `max_steps` | Limits provider steps |
| `max_cost_usd` | Limits reported cost |

## Example

```baml
class Resolution {
  reply: string,
}

function transfer_to_human(reason: string) -> string {
  `queued: ${reason}`
}

function ResolveTicket(message: string) -> Resolution {
  provider: "openai/gpt-5.6-luna"
  prompt: `
    Resolve this ticket. Transfer it when human judgment is required.

    ${message}

    ${ctx.output_format}
  `
  tools: [
    ai.tool(transfer_to_human).as_handoff(),
  ]
}

let outcome = ResolveTicket.task("This dispute needs a supervisor.").run(
  runner = ai.run.Agent.new(
    max_steps = 8,
    max_cost_usd = 0.25,
  ),
)
```

| Outcome | Contains |
| --- | --- |
| `BudgetReached` | Conversation, reason, and steps taken |
| `Handoff` | Target, arguments, conversation, and steps taken |

Limits are checked between provider steps. They do not interrupt an in-flight
provider request or half-executed tool. Both outcomes retain enough
conversation state for the application to resume or inspect the run.

[Back to tools and agents](../tools-and-agents.md)
