# Fall back between providers

Fallback tries compatible providers in order while preserving the task's
declared result type.

## Utilities used

| Utility | Purpose |
| --- | --- |
| `ai.run.Fallback` | Tries providers in order |
| `task.with_provider(...)` | Re-renders for each selected provider |

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

let resolution = ResolveTicket.task("I was charged twice.").run(
  runner = ai.run.Fallback.new(
    runner = ai.run.Completion.new(),
    providers = [
      FastModel,
      CarefulModel,
    ],
  ),
)
```

Before each attempt, fallback rebinds and re-renders the task for that provider.
It continues only after a failure that is both retryable and safe to replay.

Fallback is not load balancing. It is a visible ordered recovery policy. A
router chooses before execution; fallback reacts after an attempt fails.

[Back to routing and reliability](../routing-and-reliability.md)
