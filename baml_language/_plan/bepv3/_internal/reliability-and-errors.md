# Reliability and error facts

Retry and fallback are runner composition. They need structured facts about
what happened, not guesses based on an error string.

## Failure facts

Provider and runner failures expose:

```baml
class FailureFacts {
  phase: ai.FailurePhase,
  retryable: bool,
  side_effect: ai.SideEffectState,
  request_id: string?,
  retry_after: duration?,
}
```

`SideEffectState` is:

```baml
enum SideEffectState {
  NotStarted
  Started
  Unknown
}
```

`Unknown` must be handled like `Started` for replay safety.

Useful phases include:

- local validation;
- request construction;
- connect;
- request accepted;
- response headers;
- response body;
- stream yielded;
- tool invocation;
- remote job submitted; and
- result decoding.

## Retry runner

`Retry<Inner>` retries the whole inner operation only when:

- the inner error reports `retryable = true`;
- no observable output has been yielded;
- side effects are known not to have started; and
- the configured attempt and time limits allow it.

The runner delegates successful output unchanged:

```text
Inner.Output == Retry<Inner>.Output
```

It may widen the error set with `RetryExhausted`.

## Fallback runner

`Fallback<Inner>` runs the same lifecycle across providers:

```baml
ai.run.Fallback.new(
  runner = ai.run.Completion.new(),
  providers = [Primary, Secondary],
)
```

It must check that every provider satisfies the inner runner's capability
constraint.

Fallback may move to the next provider only while replay is safe. A provider
timeout after accepting a remote job is not equivalent to a local connection
failure.

## Agent failures

The Agent applies retry policy at the smallest safe boundary:

- a provider step may be retried if no response or tool call was observed;
- invalid tool arguments may be returned to the model for repair;
- a tool invocation is not repeated once its side effect may have started;
- a continuation request may be retried only if provider idempotency permits;
  and
- completed earlier Agent steps are not replayed merely because a later step
  failed.

Agent limits produce `BudgetReached`, not a generic failure.

## Streaming boundary

Once a stream yields an item, switching providers or replaying the operation
would duplicate or contradict observable output. Therefore:

```text
before first yield → retry/fallback may be allowed
after first yield  → surface terminal stream failure
```

The stream's terminal error carries usage and provider facts accumulated so
far.

## Idempotency

Providers and remote resources may accept an idempotency key. BAML generates
one per logical operation and keeps it stable across a retry that is declared
safe.

A new logical application request receives a new key. A fallback provider does
not receive a key from another provider unless both implementations explicitly
share that idempotency domain.

Application tools may declare their own replay policy. The default for an
arbitrary BAML function is non-replayable after invocation starts.

