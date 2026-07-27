# Move messages between providers

Provider switching creates a new conversation owned by the destination
provider.

## Utilities used

| Utility | Purpose |
| --- | --- |
| `conversation.messages()` | Portable message projection |
| `ConversationImportProvider.import_messages` | Creates destination state |
| `ConversationImport` | Reports fidelity and warnings |

## Example

```baml
class Answer {
  text: string,
}

function ContinueSupport(instruction: string) -> Answer {
  provider: CarefulModel
  prompt: `
    Continue this support conversation.

    ${instruction}

    ${ctx.output_format}
  `
}

let imported = CarefulModel.import_messages(
  existing_conversation.messages(),
);

log.info(imported.fidelity);
log.info(imported.warnings);

let answer = ContinueSupport.task("Give the final recommendation.").run(
  runner = ai.run.Agent.new(
    conversation = imported.conversation,
  ),
)
```

| Fidelity | Meaning |
| --- | --- |
| `Exact` | No required provider state was lost |
| `MessagesOnly` | Visible messages were preserved |
| `Lossy` | Some visible or provider-specific detail was approximated |

The returned conversation reports `CarefulModel` as its owner. A wrapper must
not return a conversation owned by an unrelated inner provider.

[Back to conversations and state](../conversations-and-state.md)
