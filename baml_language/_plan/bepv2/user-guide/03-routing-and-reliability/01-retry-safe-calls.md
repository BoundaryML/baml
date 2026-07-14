# Retry a replay-safe call

Retry combines facts from the failure with replay policy from the operation.
Neither side can decide safely by itself.

## Use it

```baml
let ReliableModel = FastModel.with_retry(ai.RetryPolicy {
  max_attempts: 3,
  base_delay: baml.time.Duration.from_milliseconds(200),
})

let resolution = ResolveTicket(ticket, $provider = ReliableModel)
```

The wrapper records each attempt, respects provider `retry_after`, and replays
only when `ai.may_replay` permits it.

## Decision inputs

```text
failure:  kind + commit state + optional retry-after
operation: Safe | RequiresIdempotencyKey | Never
decision: ai.may_replay(operation, failure)
```

A rate limit known to be `NotCommitted` is a normal retry candidate. An invalid
request is futile. An agentic `drive` defaults to `Never` because tools may
have produced effects.

## Why not hand-write the loop

A generic wrapper consistently handles backoff, commit state, streaming
initiation, background idempotency, and attempt telemetry. A local loop tends
to forget one of those boundaries.

## Test it

Inject two classified rate-limit failures followed by a deterministic success.
Assert three attempts and one returned `Resolution`. Also test that an
`InvalidRequest` stops after one attempt.

## Related design and scenarios

- [Retry](../../pages/08-reliability-and-errors.md#retry)
- Scenario 29 reliability

