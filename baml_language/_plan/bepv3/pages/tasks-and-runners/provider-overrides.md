# Override a provider

The same LLM function may be rebound to another compatible provider.

## Utilities used

| Utility | Purpose |
| --- | --- |
| `$provider` | Compiler-injected direct-call and task override |
| `task.with_provider(...)` | Rebinds an existing task |

## Example

```baml
class Classification {
  category: string,
  confidence: float,
}

function ClassifyTicket(message: string) -> Classification {
  provider: "openai/gpt-5.6-luna"
  prompt: `
    Classify this support message.

    ${message}

    ${ctx.output_format}
  `
}

let careful = ai.Anthropic {
  model: "claude-sonnet-4-6",
  api_key: baml.env.get_or_panic("ANTHROPIC_API_KEY"),
  base_url: null,
  extra_headers: null,
  extra_body: null,
};

let direct = ClassifyTicket(
  "I was charged twice.",
  $provider = careful,
);

let rebound = ClassifyTicket
  .task("I was charged twice.")
  .with_provider(careful)
  .run(runner = ai.run.Completion.new())
```

Rebinding re-renders provider-sensitive prompt details, including output
instructions and message layout. It does not mutate the original task.

A provider override may change cost, latency, supported capabilities, native
tools, and data handling. Treat it as an execution decision rather than a
cosmetic model name change.

[Back to tasks and runners](../tasks-and-runners.md)
