# Evaluate provider quality

> **Status:** Implemented in the executable reference.

An evaluation compares useful behavior, not merely schema validity.

## Fixed dataset

```baml
class EvalCase {
  ticket: Ticket,
  expected_intent: Intent,
  required_facts: string[],
}

class EvalResult {
  provider: string,
  correct_intent: bool,
  grounded: bool,
  score: float,
  latency_ms: int,
  usage: ai.Usage?,
}
```

Run each provider over the same cases and capture the full response metadata.
Use deterministic checks where possible and a separately declared judge task
only for genuinely subjective criteria.

## Judge without hiding production failures

```baml
function JudgeResolution(
  ticket: Ticket,
  candidate: Resolution,
  rubric: string,
) -> EvalVerdict {
  provider: JudgeModel
  prompt: `Evaluate ${candidate} for ${ticket} using ${rubric}. ${ctx.output_format}`
}
```

Store candidate-provider usage separately from judge usage. A judge parse
failure is an evaluation failure, not evidence that the candidate was bad.

## Report

Compare validity, task success, latency, attempts, tokens, and cost. Avoid one
opaque aggregate score that hides which tradeoff changed.
