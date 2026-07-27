# Switch provider after failure

A failure-aware runner may move to another provider when replay is safe.

## Utilities used

| Utility | Purpose |
| --- | --- |
| `ai.Failure` | Describes retryability and effects |
| `ai.run.Fallback` | Applies the ordered recovery policy |
| `ReplayPolicy` | Protects against unsafe repetition |

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

let response = ResolveTicket.task("I was charged twice.").run(
  runner = ai.run.Fallback.new(
    runner = ai.run.CompletionWithMeta.new(),
    providers = [FastModel, CarefulModel],
  ),
)
```

A rate limit before any effect may continue to the next provider. A failure
after an application tool changed business state may not.

Failure types report facts. The runner combines those facts with the
operation's replay policy. A provider does not unilaterally decide whether the
application is willing to repeat an effect.

[Back to routing and reliability](../routing-and-reliability.md)
