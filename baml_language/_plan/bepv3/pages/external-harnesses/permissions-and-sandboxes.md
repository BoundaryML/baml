# Harness permissions and sandboxes

Permissions are explicit harness configuration, not prompt suggestions.

## Utilities used

| Utility | Purpose |
| --- | --- |
| `permission_mode` | Controls approval behavior |
| `sandbox` | Limits filesystem and process access |
| `allowed_tools` | Narrows harness capabilities |

## Example

```baml
class RepositoryReport {
  cause: string,
  recommendation: string,
}

function InvestigateRepository(issue: string) -> RepositoryReport {
  provider: CodingModel
  prompt: `
    Investigate this repository issue without changing files.

    ${issue}

    ${ctx.output_format}
  `
}

let run = InvestigateRepository.task(issue).run(
  runner = ai.run.Harness<RepositoryReport>.new(
    harness = ClaudeCode,
    cwd = "/workspace",
    permission_mode = "read-only",
    sandbox = "workspace",
    allowed_tools = ["read", "search", "test"],
  ),
)
```

The adapter maps these portable settings to the harness's exact protocol. It
must fail clearly when it cannot enforce a requested boundary.

[Back to external harnesses](../external-harnesses.md)
