# Observability and testing

Typed responses and events make provider behavior visible without mixing
telemetry with execution policy.

## Utilities used

| Utility | Purpose |
| --- | --- |
| `Response<T>` | Typed value plus provider metadata |
| `AgentObserver` | Receives read-only Agent events |
| `test` and `assert.*` | Ordinary typed BAML tests |

## Example

```baml
class Classification {
  category: string,
}

function ClassifyTicket(message: string) -> Classification {
  provider: "openai/gpt-5.6-luna"
  prompt: `
    Classify this support ticket.

    ${message}

    ${ctx.output_format}
  `
}

let response = ClassifyTicket.task("I was charged twice.").run(
  runner = ai.run.CompletionWithMeta.new(),
);

log.info({
  "provider": response.meta.provider,
  "request_id": response.meta.request_id,
  "usage": response.meta.usage,
})
```

Observers report what happened. Hooks decide what should happen. Keeping those
roles separate lets the same telemetry work in production, tests, and evals.

## Continue

- [Observe an Agent](./observability-and-testing/observe-an-agent.md)
- [Usage and cost](./observability-and-testing/usage-and-cost.md)
- [Test across providers](./observability-and-testing/test-across-providers.md)
- [Evaluate provider quality](./observability-and-testing/evaluate-provider-quality.md)
