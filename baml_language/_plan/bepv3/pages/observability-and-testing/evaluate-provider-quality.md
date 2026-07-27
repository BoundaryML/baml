# Evaluate provider quality

An evaluation is normal typed code over task results, metadata, and expected
properties.

## Utilities used

| Utility | Purpose |
| --- | --- |
| `Response<T>` | Candidate value and metadata |
| BAML test and assertions | Defines deterministic expectations |
| Optional judge LLM function | Scores qualities requiring model judgment |

## Example

```baml
class Resolution {
  reply: string,
  resolved: bool,
}

class QualityScore {
  accurate: bool,
  helpfulness: int,
  explanation: string,
}

function ResolveTicket(message: string) -> Resolution {
  provider: CandidateModel
  prompt: `
    Resolve this support ticket.

    ${message}

    ${ctx.output_format}
  `
}

function JudgeResolution(
  message: string,
  resolution: Resolution,
) -> QualityScore {
  provider: JudgeModel
  prompt: `
    Evaluate this support resolution.

    Message: ${message}
    Resolution: ${resolution}

    ${ctx.output_format}
  `
}

let candidate = ResolveTicket("I was charged twice.");
let score = JudgeResolution("I was charged twice.", candidate)
```

The candidate and judge are separate LLM functions with separate providers and
typed outputs. Usage and cost can be aggregated without becoming part of
conversation state.

[Back to observability and testing](../observability-and-testing.md)
