# Routing and reliability

Retries, fallback, and routing decide whether to repeat or redirect semantic
model work. They are runner policy, not provider transport details.

## Utilities used

| Utility | Purpose |
| --- | --- |
| `ai.run.Retry` | Repeats a replay-safe runner operation |
| `ai.ReplayPolicy` | Describes whether replay is allowed |
| `ai.Failure` | Exposes facts used by reliability policy |

## Example

```baml
class Classification {
  category: string,
}

function ClassifyTicket(message: string) -> Classification {
  provider: "openai/gpt-5.6-luna"
  prompt: `
    Classify this support message.

    ${message}

    ${ctx.output_format}
  `
}

let response = ClassifyTicket.task("I was charged twice.").run(
  runner = ai.run.Retry.new(
    runner = ai.run.CompletionWithMeta.new(),
    max_attempts = 3,
  ),
)
```

The retry runner preserves the inner runner's output type:

```text
CompletionWithMeta<T> → Response<T>
Retry<CompletionWithMeta<T>> → Response<T>
```

It retries only when both are true:

1. The failure is retryable.
2. Replaying the operation is safe.

Transport retry inside one provider request remains provider-owned. Repeating
the whole semantic operation belongs to the runner.

## Continue

- [Fallback between providers](./routing-and-reliability/fallback-between-providers.md)
- [Route before running](./routing-and-reliability/route-before-running.md)
- [Switch provider between turns](./routing-and-reliability/switch-provider-between-turns.md)
- [Switch provider after failure](./routing-and-reliability/switch-provider-after-failure.md)
- [Side effects and idempotency](./routing-and-reliability/side-effects-and-idempotency.md)
- [Streaming failure boundaries](./routing-and-reliability/streaming-failure-boundaries.md)
