# Observe harness events

One harness runner can expose external progress while preserving a final typed
value. An event callback does not require a second streaming runner.

## Utilities used

| Utility | Purpose |
| --- | --- |
| `ai.run.Harness<T>` | Runs the harness and accepts `on_event` |
| `ai.AgentEvent` | Normalized text, tool, usage, and terminal events |
| `HarnessRun<T>` | Final typed value plus the retained event history |

## Example

```baml
class RepositoryReport {
  cause: string,
  recommendation: string,
}

function InvestigateRepository(issue: string) -> RepositoryReport {
  provider: CodingModel
  prompt: `
    Investigate this repository issue.

    ${issue}

    ${ctx.output_format}
  `
}

let kinds: string[] = []

let run = InvestigateRepository.task(issue).run(
  runner = ai.run.Harness<RepositoryReport>.new(
    harness = ClaudeCode,
    cwd = "/workspace",
    on_event = (event) -> {
      kinds.push(event.kind())
      ui.show(event)
    },
  ),
)

assert.equal(kinds, run.events.map((event) -> { event.kind() }))
let report = run.value
```

Adapters retain provider- or runtime-specific raw payloads as optional
diagnostic data while emitting normalized typed events for portable
applications. Use the separate `HarnessSession` API for steering,
interruption, save, or resume; listening alone does not require a session
handle.

[Back to external harnesses](../external-harnesses.md)
