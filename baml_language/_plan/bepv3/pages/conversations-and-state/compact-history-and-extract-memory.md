# Compact history and extract memory

Applications may replace a long editable history with a shorter summary.

## Utilities used

| Utility | Purpose |
| --- | --- |
| `MessageHistory` | Portable source history |
| Normal LLM function | Produces typed memory |
| `ConversationFidelity.Lossy` | Describes lost exact detail |

## Example

```baml
class Memory {
  customer_goal: string,
  facts: string[],
  unresolved_questions: string[],
}

function ExtractMemory(history: ai.MessageHistory) -> Memory {
  provider: "openai/gpt-5.6-luna"
  prompt: `
    Extract durable memory from this support conversation.

    ${history}

    ${ctx.output_format}
  `
}

let memory = ExtractMemory(long_history);

let compact = ai.MessageHistory.empty()
  .append(ai.ChatMessage.system(`Conversation memory: ${memory}`))
```

Compaction is an application decision. It does not claim to recreate
provider-private state, and it reports a lossy boundary when imported as a new
provider conversation.

Typed memory is often easier to inspect and migrate than one large summary
string.

[Back to conversations and state](../conversations-and-state.md)
