# The journal

## What the journal is

The journal is an append-only log of typed events. Every session has
exactly one. It is the source of truth: the prompt the model sees, the
snapshot string, the trace in your dashboard, and crash recovery are all
derived from it.

Every LLM call, tool call, and child session inside a session scope is
recorded automatically. You do not instrument anything. A one-shot task
records a journal too — that journal is its trace.

After one task run with two tool calls:

```
seq 0   SessionStarted  PlanTrip {"request": "2 weeks in Japan"}
seq 1   AssistantMessage   provider=openai/gpt-5.2  (decided to call a tool)
seq 2   ToolRequested   t1 search_flights {"origin":"SFO","dest":"NRT"}
seq 3   Usage           in=812 out=41
seq 4   ToolCompleted   t1 [{"airline":"ANA","price":890.0}, ...]
seq 5   AssistantMessage   provider=openai/gpt-5.2
seq 6   ToolRequested   t2 search_hotels {"city":"Kyoto","max_nightly":150.0}
seq 7   Usage           in=1204 out=38
seq 8   ToolCompleted   t2 [...]
seq 9   FinalProduced   {"destination":"Japan","days":14,...}
seq 10  Usage           in=1651 out=402
```

What the journal is not:

- Not a message array. Provider messages are a *rendering* of the journal
  (the transcript), produced per provider by the client.
- Not a stream of token deltas. Streaming travels on an ephemeral
  channel; the journal records final messages.
- Not a place for logs. `log.info` goes to the runtime's log stream.

## Built-in events

```baml
type Event = SessionStarted | UserMessage | AssistantMessage
           | ToolRequested | ToolCompleted | ToolFailed
           | FinalProduced | ToolsChanged | ClientChanged | PolicyChanged
           | StepCompleted | ChildSpawned | ChildFinished
           | Interrupted | Usage | Compacted
```

| Event | Recorded when |
|---|---|
| `SessionStarted` | The session is created. Carries the function name and arguments. |
| `UserMessage` | A message is injected into the conversation. |
| `AssistantMessage` | The model produces a message. Canonical content + raw provider payload + provider ID. |
| `ToolRequested` / `ToolCompleted` / `ToolFailed` | Tool lifecycle. |
| `FinalProduced` | The model produces the function's return type. |
| `ToolsChanged` | The mounted toolbox changes. |
| `ClientChanged` / `PolicyChanged` | A setter changed the session's client or policy (`03_configuration.md`). |
| `StepCompleted` | A durable step inside a tool commits (`12_durability.md`). |
| `ChildSpawned` / `ChildFinished` | A child session starts / returns. |
| `Interrupted` | An interrupt took effect. |
| `Usage` | Token counts for one provider call. |
| `Compacted` | Older entries were summarized. |

Matching over events requires a `_` arm; new built-in events may be added
in future releases.

`AssistantMessage` records both the canonical fields and the raw provider
payload, plus which client produced it. Clients use this for
same-provider fidelity (`05_models.md`). `Compacted { summary,
through_seq }` changes rendering, not history: replaced entries stay in
the journal.

By default the transcript renders `UserMessage`, `AssistantMessage`, tool
events, and compaction summaries. Everything else is journal-only unless
opted in (see `Promptable` below).

## Custom events

A custom event is a class. Define events at module level, widen the
built-in union, and bind the union on your policy — the session infers it
from there:

```baml
class PermissionRequested { call_id: string, tool: string, why: string }
class PermissionGranted   { call_id: string }
class TodoUpdated         { items: string[] }

type ReleaseEvent = baml.session.Event
                  | PermissionRequested | PermissionGranted | TodoUpdated
```

```baml
class ReleasePolicy {
    inner: baml.session.Policy,
    implements baml.session.Policy {
        type Ev = ReleaseEvent
        function update(self, st: SessionState, j: Journal<ReleaseEvent>, e: ReleaseEvent) -> Command[] { /* ... */ }
    }
}

function ReleaseAgent(goal: string) -> Report {
    client: "anthropic/claude-sonnet-5"
    tools: [request_approval, run_bash]
    prompt: `You are a release agent. ${goal} ${ctx.transcript} ${ctx.output_format}`
}

let s = ReleaseAgent@session(goal = g, $policy = ReleasePolicy { inner: baml.session.ToolLoop { max_steps: 50 } });
s.send(PermissionGranted { call_id: "t7" });   // typed by the union
```

Three producers can append custom events:

```baml
s.send(PermissionGranted { call_id: id });                    // 1. the application
j.append(PermissionRequested { call_id: c, tool: t, why: w }); // 2. the policy, in update
baml.session.emit(TodoUpdated { items: items });               // 3. a tool, ambient — no extra params
```

Clients never produce custom events; the runner treats them opaquely.
Custom events are invisible to the model unless the class implements
`Promptable`:

```baml
class TodoUpdated {
    items: string[],
    implements baml.session.Promptable {
        function to_prompt(self) -> string? { `(todo list: ${self.items.join(", ")})` }
    }
}
```

## Journal stores

Sessions used as values keep the journal in memory; `snapshot()` moves it
out of the process. Named instances (`$id = ...`) and jobs read
and write through a store:

```baml
interface JournalStore {
    function append(self, session_id: string, batch: Entry[]) -> void   // atomic per batch
    function read(self, session_id: string, from_seq: int = 0) -> Entry[]
}
```

Built-in: in-memory and file-backed. Bring your own for Postgres or
anything else — the interface is two functions. `append` takes a batch
because events from one turn commit atomically (`12_durability.md`).

Tail a journal by polling past your last seen sequence number, or over
SSE on served sessions (`13_serving.md`).

## Compaction

Long sessions outgrow context windows. A policy decides to compact
(usually `with_compaction` middleware watching `Usage`); the runner calls
a summarizer and appends `Compacted { summary, through_seq }`; from then
on clients render the summary in place of the replaced prefix. Nothing is
deleted — compaction is a recorded rendering instruction.
