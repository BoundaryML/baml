# Conversations and state

Applications need editable messages. Providers need exact continuation state.
Those are related, but they are not the same value.

## Utilities used

| Utility | Purpose |
| --- | --- |
| `ai.MessageHistory` | Editable portable history |
| `ai.Conversation` | Exact provider-owned continuation |
| `ai.run.Agent` | Resumes from a conversation |

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

let history = ai.MessageHistory.empty()
  .append(ai.ChatMessage.user("My order is late."))
  .append(ai.ChatMessage.assistant("What is the order number?"))
  .append(ai.ChatMessage.user("order-42"));

let answer = AnswerQuestion.task("What should we do next?")
  .with_messages(history)
  .run(runner = ai.run.Completion.new())
```

`MessageHistory` is ordinary application data. It may be edited, displayed,
stored, forked, or compacted.

A provider `Conversation` may also contain call IDs, encrypted reasoning,
cache handles, or remote continuation IDs. It exposes messages as a portable
view but retains exact provider state.

## Continue

- [Compact history and extract memory](./conversations-and-state/compact-history-and-extract-memory.md)
- [Fork a conversation](./conversations-and-state/fork-a-conversation.md)
- [Resume an Agent](./conversations-and-state/resume-an-agent.md)
- [Move messages between providers](./conversations-and-state/move-messages-between-providers.md)
- [Save and restore a conversation](./conversations-and-state/save-and-restore-a-conversation.md)
- [Provider-owned sessions](./conversations-and-state/provider-owned-sessions.md)
