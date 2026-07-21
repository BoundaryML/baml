# Implement a harness adapter

> **Status:** Partial — out-of-body harness composition is implemented. A
> persistent external-process transport is still host-runtime work.

Normalize a harness's native protocol into semantic sessions, events, and one
typed terminal outcome. JSONL, JSON-RPC, HTTP, and language generators remain
private implementation choices.

## Capability shape

```baml
interface Harness {
  function label(self) -> string throws never { "harness" }
  function open(self, options: ai.HarnessOptions) -> ai.HarnessSession
    throws baml.errors.UnknownError
  function run<T>(self, session: ai.HarnessSession, task: ai.Task<T>)
    -> ai.HarnessRun<T> throws baml.errors.UnknownError
  function stream<TPartial, T>(
    self,
    session: ai.HarnessSession,
    task: ai.Task<T>,
  ) -> ai.HarnessEventStream<T> throws baml.errors.UnknownError
  function save_session(self, session: ai.HarnessSession)
    -> ai.HarnessSessionToken throws baml.errors.UnknownError
  function restore_session(self, token: ai.HarnessSessionToken)
    -> ai.HarnessSession throws baml.errors.UnknownError
  function steer(self, session: ai.HarnessSession, instruction: string) -> null
    throws baml.errors.UnknownError
  function interrupt(self, session: ai.HarnessSession) -> null
    throws baml.errors.UnknownError
}
```

## Bind private state with an out-of-body implementation

```baml
class ClaudeJsonl {
  binary: string,
  workspace: string,
}

class ClaudeSession {
  transport: HostProcessTransport,
  conversation_id: string,
  history: ai.Conversation,
  is_stopped: bool,

  // cleanup must be a direct method to participate in BAML finalization. The
  // same method satisfies Resource through the empty implements block.
  function cleanup(self) -> null throws never {
    if (!self.is_stopped) {
      self.transport.stop()
      self.is_stopped = true
    }
  }

  implements ai.HarnessSession {
    function id(self) -> string throws never { self.conversation_id }
    function conversation(self) -> ai.Conversation throws never { self.history }
    function stopped(self) -> bool throws never { self.is_stopped }
  }
  implements ai.Resource {}
}

implements Harness for ClaudeJsonl {
  function open(self, options: ai.HarnessOptions) -> ai.HarnessSession {
    // Ask the host transport to spawn the process and negotiate the protocol.
  }

  function stream<TPartial, T>(
    self,
    session: ai.HarnessSession,
    task: ai.Task<T>,
  ) -> ai.HarnessEventStream<T> {
    // Render task, write JSONL, decode frames, preserve tool IDs and metadata.
  }

  // run/save/restore/steer/interrupt follow the same owned session.
}
```

`HostProcessTransport` stands for an adapter-private host transport resource;
BAML's
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
session token round-trip, interrupt/cleanup behavior, and malformed-frame
failures.
