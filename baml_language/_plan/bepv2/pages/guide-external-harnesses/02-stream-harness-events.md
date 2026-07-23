# Stream external harness events

> **Status:** Implemented in the executable reference.

A rich consumer preserves tool cards, reasoning summaries, usage, and control
events instead of projecting the harness response to final text.

## Open a streamed run

```baml
let session = CodeHarness.open(ai.HarnessOptions { cwd: "/workspace" })
defer { session.cleanup() }

let stream = CodeHarness.stream<Patch$stream, Patch>(
  session,
  FixRepository.task("inspect the test suite"),
)
defer { stream.cleanup() }

while (true) {
  match (stream.next()) {
    let event: ai.AgentEvent => render_harness_event(event),
    let done: baml.stream.StreamFinished => break,
  }
}

let run = stream.final()
```

## Event handling

```baml
function render_harness_event(event: ai.AgentEvent) -> null {
  match (event) {
    let e: ai.TextDeltaEvent => ui.append(e.text),
    let e: ai.ToolCallEvent => ui.update_tool_card(e.phase, e.call, e.result),
    let e: ai.UsageEvent => ui.update_usage(e.usage),
    _ => log.debug(event.kind()),
  }
}
```

The returned `HarnessRun<Patch>` remains authoritative for the terminal outcome.
The event log is observation, not reconstructed continuation state.
