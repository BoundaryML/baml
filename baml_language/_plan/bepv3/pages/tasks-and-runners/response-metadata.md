# Keep response metadata

Use a metadata runner when the application needs the provider request ID,
model, finish reason, or usage alongside the typed value.

## Utilities used

| Utility | Purpose |
| --- | --- |
| `ai.run.CompletionWithMeta` | Returns `Response<T>` |
| `ai.Response<T>` | Keeps `value` and `meta` together |
| `ai.Meta` | Describes the provider operation |

## Example

```baml
class Resolution {
  reply: string,
  resolved: bool,
}

function ResolveTicket(message: string) -> Resolution {
  provider: "openai/gpt-5.6-luna"
  prompt: `
    Resolve this support ticket.

    ${message}

    ${ctx.output_format}
  `
}

let response: ai.Response<Resolution> = ResolveTicket
  .task("Where is order order-42?")
  .run(
    runner = ai.run.CompletionWithMeta.new(),
  );

log.info({
  "provider": response.meta.provider,
  "model": response.meta.model,
  "request_id": response.meta.request_id,
  "usage": response.meta.usage,
});

let resolution: Resolution = response.value;
```

## Common metadata

| Field | Meaning |
| --- | --- |
| `provider` | Provider family that handled the operation |
| `model` | Provider model identifier |
| `request_id` | Provider request or operation ID, when reported |
| `finish_reason` | Why the provider stopped |
| `usage` | Input, output, cached tokens, and reported cost |
| `attributes` | Additional typed or JSON-safe provider details |

Metadata is diagnostic information, not continuation state. Exact state lives
in a `Conversation`, `Job`, `Session`, or another resource.

[Back to tasks and runners](../tasks-and-runners.md)
