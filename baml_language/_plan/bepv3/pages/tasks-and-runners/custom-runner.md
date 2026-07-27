# Write a custom runner

A runner is a configured class with an exact input, output, and error type.

## Utilities used

| Utility | Purpose |
| --- | --- |
| `ai.Runner<Input>` | Common execution protocol |
| Associated `Output` | Determines what `Task.run` returns |
| Associated `Error` | Determines what the call may throw |

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

class Audited<T, P extends ai.Provider> {
  inner: ai.Runner<
    ai.Task<T, P>,
    Output = T,
    Error = ai.CallError | baml.errors.UnknownError,
  >,
  label: string,

  implements ai.Runner<ai.Task<T, P>> {
    type Output = T
    type Error = ai.CallError | baml.errors.UnknownError

    function run(
      self,
      task: ai.Task<T, P>,
    ) -> T throws ai.CallError | baml.errors.UnknownError {
      log.info(`starting ${self.label}`);
      let value = self.inner.run(task);
      log.info(`finished ${self.label}`);
      value
    }
  }
}

let summary = Summarize.task(article).run(
  runner = Audited<Summary, ai.OpenAi> {
    inner: ai.run.Completion.new(),
    label: "article-summary",
  },
)
```

The runner stores its own configuration and implements the protocol inside
the class. A detached implementation is useful when adapting a type you do not
own, but co-locating it is clearer for a class designed as a runner.

The `inner` field pins its associated `Output` and `Error` types. The wrapper
therefore keeps the same result type without putting differently typed runners
in an erased registry.

Custom runners compose without a compiler plugin. Editor completion and
interface-implementor search provide discovery without a runtime registry
that erases associated types.

[Back to tasks and runners](../tasks-and-runners.md)
