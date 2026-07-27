# Add a provider capability

A custom capability is an interface plus a runner or direct resource
operation.

## Utilities used

| Utility | Purpose |
| --- | --- |
| Capability interface | Describes one provider operation |
| Runner | Chooses the portable lifecycle and output |
| Associated output | Preserves the task result type |

## Example

```baml
class Moderated<T> {
  value: T,
  categories: string[],
}

interface ModerationProvider requires ai.Provider {
  function generate_moderated<T>(
    self,
    task: ai.Task<T>,
  ) -> Moderated<T>
}

class Answer {
  text: string,
}

function AnswerQuestion(question: string) -> Answer {
  provider: ModeratedModel
  prompt: `
    Answer this question.

    ${question}

    ${ctx.output_format}
  `
}

class ModeratedGeneration<T> {
  provider: ModerationProvider,

  implements ai.Runner<ai.Task<T>> {
    type Output = Moderated<T>
    type Error = baml.errors.Unsupported

    function run(
      self,
      task: ai.Task<T>,
    ) -> Moderated<T> {
      self.provider.generate_moderated(
        task.with_provider(self.provider),
      )
    }
  }
}

let answer = AnswerQuestion.task(question).run(
  runner = ModeratedGeneration<Answer> {
    provider: ModeratedModel,
  },
)
```

The compiler does not need to know the word "moderated." The interface,
runner, and associated type are enough.

The runner stores a provider already narrowed to `ModerationProvider`, so an
incompatible provider is a construction-time type error rather than a runtime
branch.

[Back to build your own](../build-your-own.md)
