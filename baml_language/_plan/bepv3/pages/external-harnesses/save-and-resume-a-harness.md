# Save and resume a harness

Harness sessions may cross process boundaries through opaque tokens.

## Utilities used

| Utility | Purpose |
| --- | --- |
| `save_session` | Produces a serializable token |
| `restore_session` | Reconnects to harness state |
| `HarnessSessionToken` | Stable runtime coordinates |

## Example

```baml
class RepositoryReport {
  cause: string,
  recommendation: string,
}

function InvestigateRepository(issue: string) -> RepositoryReport {
  provider: CodingModel
  prompt: `
    Continue investigating this repository issue.

    ${issue}

    ${ctx.output_format}
  `
}

let token = ClaudeCode.save_session(session);
database.save("issue-42", baml.json.stringify(token));

let restored = ClaudeCode.restore_session(
  baml.json.from_string<ai.HarnessSessionToken>(
    database.load("issue-42"),
  ),
);

defer { restored.close() }

let run = restored.run(
  InvestigateRepository.task("Continue from the saved investigation."),
)
```

The token contains no executable callbacks or secrets. The configured harness
adapter supplies credentials and validates runtime ownership when restoring.

[Back to external harnesses](../external-harnesses.md)
