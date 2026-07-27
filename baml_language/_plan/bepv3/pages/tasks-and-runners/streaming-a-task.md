# Stream a task

Streaming exposes partial values while preserving the final typed result.

## Utilities used

| Utility | Purpose |
| --- | --- |
| `ai.run.Stream<TPartial, T>` | Opens the provider stream |
| `baml.llm.Stream<TPartial, T>` | Produces partials and one final `T` |

## Example

```baml
class Draft {
  subject: string,
  body: string,
}

function DraftReply(message: string) -> Draft {
  provider: "openai/gpt-5.6-luna"
  prompt: `
    Draft a helpful support reply.

    ${message}

    ${ctx.output_format}
  `
}

let stream = DraftReply.task("My package is late.").run(
  runner = ai.run.Stream<DraftPartial, Draft>.new(),
);

for (let partial in stream) {
  ui.show_draft(partial)
}

let final: Draft = stream.final()
```

The final value is parsed against the same `Draft` contract used by a normal
call. A partial value is a view of progress, not a weaker final schema.

## Failure boundary

Before the first partial, a safe retry may replay the whole operation. After a
partial becomes observable, replay could duplicate text or side effects. The
stream records whether anything has been observed so reliability policy can
make that distinction.

Streaming provider support is an explicit capability. A provider that only
supports bounded completion cannot be passed to `ai.run.Stream`.

[Back to tasks and runners](../tasks-and-runners.md)
