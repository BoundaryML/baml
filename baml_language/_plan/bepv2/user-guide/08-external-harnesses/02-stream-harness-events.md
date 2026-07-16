# Stream external harness events

A rich consumer preserves tool cards, reasoning summaries, usage, and control
events instead of projecting the harness response to final text.

## Open a streamed run

```baml
let session = CodeHarness.open(ai.HarnessOptions { cwd: "/workspace" })
defer { CodeHarness.destroy(session) }

let stream = CodeHarness.stream<Patch>(
  session,
  FixRepository.task("inspect the test suite").with_provider(CodeHarness),
)
defer { stream.cleanup() }

while (true) {
  match (stream.next()) {
    let event: ai.AgentEvent => render_harness_event(event),
    null => break,
  }
}

let run = stream.final()
```

## Event handling

```baml
function render_harness_event(event: ai.AgentEvent) -> void {
  match (event) {
    let e: ai.TextDelta => ui.append(e.text),
    let e: ai.ReasoningDelta => ui.reasoning(e.summary, e.redacted),
    let e: ai.ToolCallStarted => ui.tool_card(e.call),
    let e: ai.ToolCallFinished => ui.finish_tool_card(e.result),
    _ => log.debug(event.kind()),
  }
}
```

The returned `AgentRun<Patch>` remains authoritative for the terminal outcome.
The event log is observation, not reconstructed continuation state.

## Related design and scenarios

- Scenarios 39 harness extensibility, 42 harness abstraction
