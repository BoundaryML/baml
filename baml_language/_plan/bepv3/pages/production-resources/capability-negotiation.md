# Negotiate capabilities intentionally

Erasing a concrete provider type also erases static proof of its capabilities.
Runtime negotiation should therefore be explicit.

## Utilities used

| Utility | Purpose |
| --- | --- |
| Interface pattern matching | Narrows an erased provider |
| `Unsupported` | Reports a missing runtime capability |

## Example

```baml
class Answer {
  text: string,
}

function AnswerQuestion(question: string) -> Answer {
  provider: DefaultModel
  prompt: `
    Answer this question.

    ${question}

    ${ctx.output_format}
  `
}

function run_best(
  provider: ai.Provider,
  question: string,
) -> Answer {
  let task = AnswerQuestion.task(question).with_provider(provider);

  match (provider) {
    let streaming: ai.StreamingProvider => {
      task.run(
        runner = ai.run.Stream<AnswerPartial, Answer>.new(),
      ).final()
    },
    let completion: ai.CompletionProvider => {
      task.run(runner = ai.run.Completion.new())
    },
    _ => throw baml.errors.Unsupported {
      message: "provider cannot answer this task",
    },
  }
}
```

Keep concrete provider types whenever possible. Use runtime negotiation at
plugin, configuration, or network boundaries where the provider genuinely is
not known statically.

[Back to production resources](../production-resources.md)
