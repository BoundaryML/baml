# Provider capabilities

A provider implements only the operations it can honestly perform.

## Utilities used

| Capability | Operation |
| --- | --- |
| `CompletionProvider` | Bounded completion response |
| `GenerationProvider` | Exactly one model interaction and response |
| `StreamingProvider` | Incremental output |
| `ToolCallingProvider` | Provider turns with application tool calls |

## Example

```baml
class Answer {
  text: string,
}

function AnswerQuestion(question: string) -> Answer {
  provider: "openai/gpt-5.6-luna"
  prompt: `
    Answer this question.

    ${question}

    ${ctx.output_format}
  `
}

let task = AnswerQuestion.task("Why is the sky blue?");

let answer = task.run(
  runner = ai.run.Generation.new(),
);

let stream = task.run(
  runner = ai.run.Stream<AnswerPartial, Answer>.new(),
)
```

Both calls are valid only when the selected provider implements their required
capability.

Capability interfaces keep unrelated promises separate. Supporting realtime
does not imply a bounded `Answer`, and supporting generation does not imply
streaming.

Task construction itself only needs a provider that can render the task. A
specific runner supplies the stronger execution requirement.

[Back to tasks and runners](../tasks-and-runners.md)
