# Implement a harness adapter

A custom adapter implements the harness protocol and stores its own
configuration.

## Utilities used

| Utility | Purpose |
| --- | --- |
| `ai.Harness` | Portable multi-operation harness interface |
| `ai.HarnessSession` | Live external session resource |
| Function fields | Inject process transport and test behavior |

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

class AcmeHarness {
  executable: string,
  send: (json) -> json throws baml.errors.Io,

  implements ai.Harness {
    function label(self) -> string {
      "acme"
    }

    function open(
      self,
      options: ai.HarnessOptions,
    ) -> ai.HarnessSession {
      acme_open(self, options)
    }

    function run<T>(
      self,
      session: ai.HarnessSession,
      task: ai.Task<T>,
      on_event: ((ai.AgentEvent) -> null throws never)?,
    ) -> ai.HarnessRun<T> {
      acme_run(
        self,
        session,
        task,
        emit = (event) -> {
          match (on_event) {
            let callback: ((ai.AgentEvent) -> null throws never) => callback(event),
            null => null,
          }
        },
      )
    }
  }
}

let run = InvestigateRepository.task(issue).run(
  runner = ai.run.Harness<RepositoryReport>.new(
    harness = AcmeHarness {
      executable: "acme-agent",
      send: acme_process_send,
    },
    cwd = "/workspace",
  ),
)
```

The adapter converts process or network events into the portable harness
model. It owns protocol framing, session identity, permission mapping, and
cleanup.

[Back to external harnesses](../external-harnesses.md)
