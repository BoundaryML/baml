# External harnesses

A coding harness may own a long-running external agent loop while still
consuming an ordinary typed task.

## Utilities used

| Utility | Purpose |
| --- | --- |
| `ai.run.Harness<T>` | Runs a task through an external harness |
| `ai.HarnessRun<T>` | Typed value, events, and resume token |
| `ai.HarnessOptions` | Permissions, sandbox, and working directory |

## Example

```baml
class RepositoryReport {
  cause: string,
  files: string[],
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

let run = InvestigateRepository.task(
  "The checkout test fails after a retry.",
).run(
  runner = ai.run.Harness<RepositoryReport>.new(
    harness = ClaudeCode,
    cwd = "/workspace",
    permission_mode = "read-only",
    sandbox = "workspace",
    on_event = (event) -> {
      log.info(event)
    },
  ),
);

log.info(run.value);
log.info(run.events)
```

The harness owns its session, tool protocol, and external process. The LLM
function still declares the typed application result.

## Continue

- [Observe harness events](./external-harnesses/observe-harness-events.md)
- [Permissions and sandboxes](./external-harnesses/permissions-and-sandboxes.md)
- [Steer and interrupt](./external-harnesses/steer-and-interrupt.md)
- [Save and resume a harness](./external-harnesses/save-and-resume-a-harness.md)
- [Implement a harness adapter](./external-harnesses/implement-a-harness-adapter.md)
