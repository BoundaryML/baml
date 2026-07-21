# Observe an agent run

> **Status:** Implemented in the executable reference.

Agents emit a richer event sequence than one response. Choose the extension
point according to whether code observes, persists, or changes behavior.

## Stream events to a UI

```baml
let stream = ai.drivers.stream_agent(task, options)

while (true) {
  match (stream.next()) {
    null => break,
    let event: ai.AgentEvent => match (event) {
      let e: ai.TextDelta => ui.append(e.text),
      let e: ai.ReasoningDelta => ui.reasoning(e.summary, e.redacted),
      let e: ai.ToolCallStarted => ui.tool_started(e.call.name),
      let e: ai.ToolCallFinished => ui.tool_finished(e.result),
      let e: ai.ProviderChanged => ui.provider_changed(e.fidelity, e.warnings),
      _ => {},
    },
  }
}
```

## Observer, recorder, or hook?

```text
AgentObserver: see immutable events for UI/telemetry
AgentRecorder: persist immutable events durably
AgentHooks:    make bounded decisions affecting future steps
```

Observers and recorders must not mutate the driver's transcript. A hook may
return a `StepPlan` or `ToolDecision`; it still does not receive ownership of
provider-private continuation state.

## Reasoning data

Expose safe summaries and redaction markers through typed blocks. Preserve
provider metadata for round-tripping, but do not ask applications to maintain
undocumented Anthropic signatures or OpenAI reasoning state.
