# Completion and generation

Completion and generation sound similar, but they make different promises.

## Utilities used

| Utility | Promise |
| --- | --- |
| `ai.run.Completion` | Finish the task as `T` using a bounded provider policy |
| `ai.run.Generation` | Perform exactly one model interaction |

## Example

```baml
class Summary {
  title: string,
  bullets: string[],
}

function Summarize(text: string) -> Summary {
  provider: "openai/gpt-5.6-luna"
  prompt: `
    Summarize this text.

    ${text}

    ${ctx.output_format}
  `
}

let task = Summarize.task(article);

let completed: Summary = task.run(
  runner = ai.run.Completion.new(),
);

let generated: Summary = task.run(
  runner = ai.run.Generation.new(),
)
```

For a simple provider, both calls may produce one request. Their contracts still
differ.

`Generation` means one model interaction. `Completion` means one bounded
operation that finishes as `T`. A provider-managed service may use its own web
search or code execution before returning from completion.

Application tools are different. When an LLM function declares BAML
functions in `tools:`, the direct call uses the BAML Agent loop rather than
asking provider completion to execute local functions.

## When to choose each

| Need | Choose |
| --- | --- |
| Ordinary application result | Direct LLM function call |
| Explicit bounded provider completion | `Completion` |
| Exactly one sample for evaluation or routing | `Generation` |
| Application tool execution | `Agent` |
| Provider request metadata | The corresponding `WithMeta` runner |

Completion must return `T` or throw. It never returns a hidden job or open
session.

[Back to tasks and runners](../tasks-and-runners.md)
