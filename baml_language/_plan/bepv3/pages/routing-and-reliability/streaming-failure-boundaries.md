# Streaming failure boundaries

Once a caller has observed a partial value, replay may duplicate visible
output.

## Utilities used

| Utility | Purpose |
| --- | --- |
| `ai.run.Stream` | Opens a typed stream |
| `StreamError` | Reports incremental failures |
| Stream observation state | Records whether output escaped |

## Example

```baml
class Draft {
  subject: string,
  body: string,
}

function DraftReply(message: string) -> Draft {
  provider: "openai/gpt-5.6-luna"
  prompt: `
    Draft a support reply.

    ${message}

    ${ctx.output_format}
  `
}

let stream = DraftReply.task("My package is late.").run(
  runner = ai.run.Stream<DraftPartial, Draft>.new(),
);

for (let partial in stream) {
  ui.show(partial)
}

let final = stream.final()
```

| Failure point | Safe default |
| --- | --- |
| Before any partial is visible | May retry when the operation is replay-safe |
| After a partial is visible | Do not replay automatically |
| After a caller effect based on a partial | Treat as effectful |

A stream retry wrapper must not invent seamless replay after output has
escaped. Applications wanting restart behavior must opt in and decide how to
replace or reconcile earlier partials.

[Back to routing and reliability](../routing-and-reliability.md)
