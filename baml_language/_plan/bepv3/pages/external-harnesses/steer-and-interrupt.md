# Steer and interrupt a harness

Steering and interruption target one concrete harness session.

## Utilities used

| Utility | Purpose |
| --- | --- |
| `HarnessSession.steer` | Adds an instruction to an active run |
| `HarnessSession.interrupt` | Requests a controlled stop |
| Session events | Confirm the resulting state |

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

let session = ClaudeCode.open(
  ai.HarnessOptions {
    cwd: "/workspace",
    permission_mode: "read-only",
    sandbox: "workspace",
  },
);

defer { session.close() }

let run = spawn {
  session.run(InvestigateRepository.task(issue))
};

session.steer("Focus on the retry state machine.");
session.interrupt()
```

Steering is not a mutation of the original task. It is an event sent to a
specific live session and recorded in that session's trace.

[Back to external harnesses](../external-harnesses.md)
