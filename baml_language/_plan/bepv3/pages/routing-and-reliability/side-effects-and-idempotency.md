# Side effects and idempotency

Retrying a read is different from retrying a refund.

## Utilities used

| Utility | Purpose |
| --- | --- |
| `ReplayPolicy` | Marks an operation safe, idempotent, or non-replayable |
| Idempotency key | Lets a remote service deduplicate an effect |
| `CannotRetry` | Explains why replay was refused |

## Example

```baml
class Resolution {
  reply: string,
}

function issue_refund(order_id: string, idempotency_key: string) -> string {
  refunds.issue(order_id, idempotency_key)
}

function ResolveTicket(message: string) -> Resolution {
  provider: "openai/gpt-5.6-luna"
  prompt: `
    Resolve this support ticket.

    ${message}

    ${ctx.output_format}
  `
  tools: [issue_refund]
}

let outcome = ResolveTicket.task("Refund order-42.").run(
  runner = ai.run.Agent.new(
    max_steps = 8,
  ),
)
```

The Agent may repair malformed arguments before the effect runs. Once
`issue_refund` succeeds, a later model failure does not make the whole run
replay-safe.

An idempotency key may let the application repeat that specific tool call, but
it does not automatically prove that every other step in the run is safe.

When policy cannot prove replay safety, it fails closed with an informative
`CannotRetry` error.

[Back to routing and reliability](../routing-and-reliability.md)
