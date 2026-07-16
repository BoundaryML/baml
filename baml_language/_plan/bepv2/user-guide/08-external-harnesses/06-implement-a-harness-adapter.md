# Implement a harness adapter

Normalize a harness's native protocol into semantic sessions, events, and one
typed terminal outcome. JSONL, JSON-RPC, HTTP, and language generators remain
private implementation choices.

## Capability shape

```baml
interface Harness {
  type HarnessSession = unknown

  function open(self, options: ai.HarnessOptions) -> Self.HarnessSession
  function run<T>(self, session: Self.HarnessSession, task: ai.Task<T>)
    -> ai.AgentRun<T>
  function stream<T>(self, session: Self.HarnessSession, task: ai.Task<T>)
    -> ai.AgentEventStream<T>
  function save_session(self, session: Self.HarnessSession)
    -> ai.HarnessSessionToken
  function restore_session(self, token: ai.HarnessSessionToken)
    -> Self.HarnessSession
  function stop(self, session: Self.HarnessSession) -> void
}
```

## Bind private state with an out-of-body implementation

```baml
class ClaudeJsonl {
  binary: string,
  workspace: string,
}

class ClaudeSession {
  process_id: string,
  conversation_id: string,
  implements ai.Resource {
    function cleanup(self) -> void { /* idempotently stop and release */ }
  }
}

implements Harness for ClaudeJsonl {
  type HarnessSession = ClaudeSession

  function open(self, options: ai.HarnessOptions) -> ClaudeSession {
    // Ask the host transport to spawn the process and negotiate the protocol.
  }

  function stream<T>(self, session: ClaudeSession, task: ai.Task<T>)
    -> ai.AgentEventStream<T> {
    // Render task, write JSONL, decode frames, preserve tool IDs and metadata.
  }

  // run/save/restore/stop follow the same owned session.
}
```

`process_id` stands for an adapter-private host transport resource; BAML's
current `baml.sys.exec` API is a bounded subprocess call, not a persistent
JSONL child-process handle. The eventual harness runtime must supply that
owned transport instead of inventing a nonexistent `baml.process` API.

## Adapter obligations

- Preserve provider-native ordering and continuation identifiers.
- Validate the terminal payload as `T`.
- Normalize text, reasoning summaries, tools, usage, and failures into typed
  events without discarding raw provider metadata.
- Reject unsupported control verbs through capability negotiation.
- Make cleanup and process termination idempotent.
- Keep credentials and undocumented signature fields out of application-owned
  conversation data.

## Test it

Use a scripted transport to verify framing, event order, typed terminal parse,
session token round-trip, stop/cleanup behavior, and malformed-frame failures.

## Related design and scenarios

- Scenarios 37–42, especially 41 deployment protocols and 42 shared abstraction
