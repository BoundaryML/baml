# Provider-owned sessions

A provider session is a resource that may hold remote state across several
tasks.

## Utilities used

| Utility | Purpose |
| --- | --- |
| `ai.open_session` | Opens a configured provider session |
| `Session.run(task)` | Runs another task in that session |
| `SessionToken` | Resumes the remote session elsewhere |

## Example

```baml
class Answer {
  text: string,
}

function AskSupport(question: string) -> Answer {
  provider: SessionModel
  prompt: `
    Answer this support question.

    ${question}

    ${ctx.output_format}
  `
}

let session = ai.open_session(SessionModel);
defer { session.close() }

let first = session.run(
  AskSupport.task("What is the return window?"),
);

let second = session.run(
  AskSupport.task("Does that apply to sale items?"),
)
```

The session owns remote identity, polling or message coordinates, and cleanup.
Tasks remain immutable descriptions of each LLM function invocation.

Use `Conversation` for exact state inside one provider interaction and
`Session` when the provider exposes a larger named or remote lifecycle.

[Back to conversations and state](../conversations-and-state.md)
