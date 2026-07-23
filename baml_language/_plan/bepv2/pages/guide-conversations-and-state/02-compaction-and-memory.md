# Compact history and extract memory

> **Status:** Implemented in the executable reference.

Application-owned history may be edited. Compaction and memory extraction are
ordinary typed tasks plus application data operations.

## Compact an old prefix

```baml
function SummarizeConversation(head: ai.Conversation) -> string {
  provider: FastModel
  prompt: `Preserve decisions, identities, and open work: ${head}`
}

function compact(history: ai.Conversation, keep_recent: int) -> ai.Conversation {
  if (history.length() <= keep_recent) { return history }

  let head = history.drop_last(keep_recent)
  let tail = history.take_last(keep_recent)
  ai.Conversation.with_summary(SummarizeConversation(head)).append_all(tail)
}
```

The recent tail remains verbatim. The summary is explicitly lossy and should
be labelled as such.

## Extract durable application memory

```baml
class Memory {
  subject: string,
  fact: string,
}

function ExtractMemories(conversation: ai.Conversation) -> Memory[] {
  provider: FastModel
  prompt: `Extract stable user facts from ${conversation}. ${ctx.output_format}`
}

let memories = ExtractMemories(history)
memory_store.upsert_all(user_id, memories)
```

Inject recalled memories into a later task as ordinary arguments. Do not edit
an opaque provider transcript to insert them.
