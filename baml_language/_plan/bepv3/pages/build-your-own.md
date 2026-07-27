# Build your own

Runners and providers are ordinary BAML interfaces. A library may add them
without adding compiler syntax.

## Utilities used

| Utility | Purpose |
| --- | --- |
| `ai.Runner<Input>` | Extensible lifecycle protocol |
| Associated types | Preserve exact output and error types |
| Inline `implements` | Keeps behavior beside configuration |

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

class Timed<T, P extends ai.Provider> {
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
      let started = baml.time.now();
      defer {
        log.info(`${self.label} took ${baml.time.now() - started}`)
      }
      self.inner.run(task)
    }
  }
}

let summary = Summarize.task(document).run(
  runner = Timed<Summary, ai.OpenAi> {
    inner: ai.run.Completion.new(),
    label: "summarize",
  },
)
```

## Continue

- [Implement a provider](./build-your-own/implement-a-provider.md)
- [Inject a provider transport](./build-your-own/inject-a-provider-transport.md)
- [Provider request middleware](./build-your-own/provider-request-middleware.md)
- [Add a provider capability](./build-your-own/add-a-provider-capability.md)
- [Create a resource](./build-your-own/create-a-resource.md)
