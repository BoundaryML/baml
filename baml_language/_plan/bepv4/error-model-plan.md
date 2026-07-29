# The `Failure | UnknownError` error model

This is the accepted BEPv4 error design for `baml_src_temp2`. It supersedes the
closed-`ai.Error` alternative in [error-redesign.md](error-redesign.md).

## Public channel

`AgentProvider` and its reliability wrappers use this inline channel.
Capability-specific protocols may narrow it:

```baml
throws root.ai.Failure | baml.errors.UnknownError
```

`Failure` is open so applications and provider packages can define statically
catchable error types:

```baml
enum Effects {
  None
  Unknown
  Committed
}

interface Failure {
  function is_transient(self) -> bool throws never
  function effects(self) -> Effects throws never
}
```

Generic policy reads only those two facts. Applications catch concrete types
when they need provider- or domain-specific data.

## Unknown errors

`baml.errors.UnknownError` is the universal wrapper for foreign values such as
transport, JSON, and subprocess errors. Its stdlib methods define the two
allowed operations:

```baml
// Propagation: preserve a known T, append to an existing wrapper, or wrap.
baml.errors.UnknownError.with_message<T>(value, "openai step")

// Recovery at a handler: recover a bare or wrapped T without double-wrapping.
baml.errors.UnknownError.from<T>(value)
```

A blind layer uses `with_message<never>`. It does not construct
`UnknownError` directly:

```baml
} catch (e) {
  let known: root.ai.Failure => throw known,
  _ => throw baml.errors.UnknownError.with_message<never>(e, "openai step"),
}
```

Wrappers remain one level deep and collect breadcrumb messages as they cross
layers. Recovery belongs at the final handler, not in an intermediate adapter.

## Default failures

The portable vocabulary is a flat list:

| Type | Classification |
| --- | --- |
| `RateLimited { provider, retry_after_ms }` | transient, `Effects.None` |
| `NetworkFailure { provider, detail }` | transient, `Effects.Unknown` |
| `InvalidRequest { provider, status_code, detail }` | terminal, `Effects.None` |
| `Refused { provider, reason }` | terminal, `Effects.None` |
| `ParseFailed { provider, raw_output }` | terminal, `Effects.None` |
| `EffectCommitted { provider, operation }` | terminal, `Effects.Committed` |
| `baml.errors.Unsupported { message }` | terminal, `Effects.None` |

`baml.errors.Unsupported` implements `ai.Failure`; there is no separate
`ai.Unsupported` class. Providers define another class implementing `Failure`
only when they need additional typed data.

`ParseFailed` retains the raw model response so callers can log it, repair it,
or deliberately re-prompt. It is never blind-retried.

## Agent provider boundary

Normal model execution uses one protocol:

```baml
interface AgentProvider requires Provider {
  function begin<T>(self, task: Task<T>) -> Conversation
    throws root.ai.Failure | baml.errors.UnknownError

  function step<T>(
    self,
    conversation: Conversation,
    tools: root.ai.tools.Tool[],
  ) -> ModelStep<T>
    throws root.ai.Failure | baml.errors.UnknownError

  function submit(
    self,
    conversation: Conversation,
    results: root.ai.tools.ToolResult[],
  ) -> Conversation
    throws root.ai.Failure | baml.errors.UnknownError
}
```

`begin` creates provider state without a model request. `step` makes exactly
one model request. `submit` validates and records correlated application-tool
results without making another model request.

A replay-safe failed `step` is transactional: it leaves `Conversation`
unchanged. Retry reuses the same pre-attempt state, so a provider must not
retain partial response IDs, pending calls, assistant content, or other state
from the failed attempt.

The Agent owns the loop and executes application tools between `step` and
`submit`. Providers and reliability wrappers never execute or replay those
tools.

## Retry and fallback

The private retry predicate is:

```baml
function _may_retry_model_step(failure: root.ai.Failure) -> bool throws never {
  if (!failure.is_transient()) {
    return false;
  }
  match (failure.effects()) {
    root.ai.Effects.Committed => return false,
    root.ai.Effects.Unknown => return false,
    root.ai.Effects.None => {},
  }
  true
}
```

There is no provider replay policy. `Effects.Unknown` and
`Effects.Committed` both fail closed.

`ai.retry` wraps only `step`. It delegates `begin` and `submit`, retries the
same unchanged conversation, and rethrows the last real failure when attempts
are exhausted.

`ai.fallback` may start the next member only when the initial `step` fails with
a replay-safe failure before any member has produced a successful model turn.
It does not intercept the first member's `begin` failure. After progress, it
keeps the selected provider and rethrows later failures.

Neither wrapper replaces the underlying error with an exhaustion summary.

## Application catch pattern

Catch most-specific first:

```baml
provider.step<Invoice>(conversation, tools) catch (e) {
  let quota: MyQuotaError => handle_quota(quota.quota_name),
  let limited: root.ai.RateLimited => backoff(limited.retry_after_ms ?? 1000),
  let failure: root.ai.Failure => {
    if (failure.is_transient()) { handle_transient(failure) }
    else { throw failure }
  },
  let unknown: baml.errors.UnknownError => {
    match (baml.errors.UnknownError.from<MyForeignError>(unknown)) {
      let known: MyForeignError => handle_foreign(known),
      let other: baml.errors.UnknownError => throw other,
    }
  },
}
```

Declared state-machine termination is a value: `Done`, `BudgetReached`, or
`Handoff`. A provider failure, parse failure, or exhausted safe recovery path
is an error.

The reader-facing guide is
[Errors and error handling](pages/errors-and-error-handling.md).
