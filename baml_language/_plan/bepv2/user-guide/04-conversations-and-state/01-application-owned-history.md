# Application-owned conversation history

Use ordinary serializable conversation data when the application owns history
and sends it on every request.

## Declare history as input

```baml
function ContinueSupport(
  conversation: ai.Conversation,
  latest: string,
) -> Resolution {
  provider: SupportModel
  prompt: `
    Continue this support conversation:
    ${conversation}

    User: ${latest}
    ${ctx.output_format}
  `
}
```

## Append returned turns

```baml
let result = ContinueSupport(history, user_text)

history = history
  .append(ai.ConversationMessage.user(user_text))
  .append(ai.ConversationMessage.assistant(result.reply))

db.save(conversation_id, baml.json.to_string(history))
```

`Conversation` is editable application data. It is suitable for databases,
UI rendering, search, redaction, and provider changes.

## What it does not preserve

Reconstructing history from assistant text may discard tool-call IDs,
reasoning signatures, encrypted blocks, citations, or server-side continuation
IDs. Use the provider's `Transcript` during a run and an opaque token for exact
resumption.

## Related design and scenarios

- [Messages and transcripts](../../pages/05-tools-and-agents.md#messages-are-an-interface)
- Scenario 17 history and sessions

