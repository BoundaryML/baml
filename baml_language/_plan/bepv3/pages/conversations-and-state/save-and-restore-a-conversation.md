# Save and restore a conversation

Providers may seal exact continuation state into an opaque token.

## Utilities used

| Utility | Purpose |
| --- | --- |
| `save_conversation` | Produces a serializable provider token |
| `restore_conversation` | Restores exact provider state |
| `ConversationToken` | Opaque, versioned coordinates |

## Example

```baml
class Resolution {
  reply: string,
}

function ResolveTicket(message: string) -> Resolution {
  provider: SupportModel
  prompt: `
    Resolve this support ticket.

    ${message}

    ${ctx.output_format}
  `
}

let token = SupportModel.save_conversation(conversation);
database.save("ticket-1042", baml.json.stringify(token));

let restored_token = baml.json.from_string<ai.ConversationToken>(
  database.load("ticket-1042"),
);

let restored = SupportModel.restore_conversation(restored_token);

let outcome = ResolveTicket.task("Continue.").run(
  runner = ai.run.Agent.new(
    conversation = restored,
  ),
)
```

A token contains stable provider identity and a format version, not a display
name used as identity. Restoration must also restore the portable
`conversation.messages()` projection; exact wire state with an empty visible
history violates the conversation contract.

[Back to conversations and state](../conversations-and-state.md)
