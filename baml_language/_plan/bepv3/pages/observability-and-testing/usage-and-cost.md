# Track usage and cost

Usage belongs to operation metadata and may be accumulated across an Agent
run.

## Utilities used

| Utility | Purpose |
| --- | --- |
| `ai.Usage` | Token and cost counters |
| `Response.meta.usage` | Usage for one bounded operation |
| `Done.meta.usage` | Cumulative Agent usage |

## Example

```baml
class Summary {
  text: string,
}

function Summarize(document: string) -> Summary {
  provider: "openai/gpt-5.6-luna"
  prompt: `
    Summarize this document.

    ${document}

    ${ctx.output_format}
  `
}

let response = Summarize.task(document).run(
  runner = ai.run.CompletionWithMeta.new(),
);

match (response.meta.usage) {
  let usage: ai.Usage => {
    log.info(`input=${usage.input_tokens}`);
    log.info(`output=${usage.output_tokens}`);
    log.info(`cost=${usage.cost_usd}`);
  },
  null => log.info("provider did not report usage"),
}
```

Missing provider usage remains `null`; the runtime does not invent exact token
or cost data. Cached input usage is reported separately when the provider
supports it.

[Back to observability and testing](../observability-and-testing.md)
