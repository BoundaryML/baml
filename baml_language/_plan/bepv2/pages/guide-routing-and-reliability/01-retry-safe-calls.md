# Retry a replay-safe call

> **Status:** Implemented in the executable reference.

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

The wrapper records each attempt and replays only when `ai.may_replay` permits
it. Unknown/foreign errors stop immediately.

## Decision inputs

```text
failure:  retryable + effectful + refusal/unsupported predicates
operation: Safe | RequiresIdempotencyKey | Never
decision: ai.may_replay(operation, failure)
```

A typed rate-limit failure that is retryable and non-effectful is a normal
candidate. An invalid request is futile. An agentic `drive` defaults to `Never` because tools may
have produced effects.

## Why not hand-write the loop

A generic wrapper consistently handles backoff, typed failure facts, streaming
initiation, background idempotency, and attempt telemetry. A local loop tends
to forget one of those boundaries.

## Test it

Inject two typed retryable failures followed by a deterministic success.
Assert three attempts and one returned `Resolution`. Also test that an
unclassified and a typed terminal failure each stop after one attempt.

## Related design


- [Retry](../specification/09-reliability-and-errors.md#retry)
