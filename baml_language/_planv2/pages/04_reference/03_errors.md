# Error reference

## The catalog

Every class implements `Failure` unless noted. All live under the
`ai.errors` namespace, mirroring `baml.errors`, except
`baml.errors.UnknownError` itself.

| Class | Fields | Retry safety | Thrown by | Condition |
|---|---|---|---|---|
| `RateLimited` | `provider: string`, `retry_after_ms: int?`, `raw_body: string?` | `Safe` | `wire.send_as`, clients | HTTP 429 or a provider throttle signal |
| `NetworkFailure` | `provider: string`, `detail: string`, `raw_body: string?` | `Safe` | `wire.send_as`, clients | transport failure, HTTP 408/5xx, or an unreadable body |
| `InvalidRequest` | `provider: string`, `status_code: int?`, `detail: string`, `raw_body: string?` | `Safe` | `wire.send_as`, clients | the provider rejected the request (other 4xx, bad auth, bad config) |
| `Refused` | `provider: string`, `reason: string`, `raw_body: string?` | `Safe` | clients | the model or provider declined to answer (`stop_reason: Refused`) |
| `ParseFailed` | `provider: string`, `raw_output: string` | `Safe` | clients, the runner | an unusable response envelope, a truncated turn (`MaxTokens`), or a final candidate that repair could not fix |
| `StepBudgetExceeded` | `steps: int` | `Safe` | the runner | `max_steps` model turns completed without a final output |
| `ToolFailedError` | `id: string`, `name: string`, `message: string`, `cause: Failure?` | `Safe` | the runner | a `Raise`-mode tool threw; the `ToolFailed` event is appended first |
| `baml.errors.UnknownError` | `message: string` | not a `Failure` | anywhere | an untyped throw crossed a boundary and was wrapped |

`Unsafe` and `Unknown` have no built-in class in this phase, because a
failed `invoke` commits nothing locally and hosted-tool side effects
arrive with a later phase. The enum ships now so that custom errors
and future classes classify against it.

## The classification table

`classify_http(provider, status_code, body)`:

| Status | Result |
|---|---|
| 429 | `RateLimited` (with `retry_after_ms` when the header is present) |
| 408, 500–599 | `NetworkFailure` |
| other 4xx | `InvalidRequest` |

The classifier attaches the response body it received as `raw_body`,
so a failure from `wire.send_as` always carries what the provider
actually said. `ParseFailed` carries the body as `raw_output`.

## Retry safety

The `Retry` wrapper retries a thrown failure when both hold:

1. `retry_safety()` returns `Safe`. This gate is not overridable.
2. The judgment accepts it. The default judgment declines `Refused`,
   `InvalidRequest`, and `ParseFailed` and accepts the rest;
   `retry_if` replaces it.

`baml.errors.UnknownError` is never retried: it does not implement
`Failure`, so no safety fact is available.

## Throwing your own

Errors are plain classes. Throw one from a tool, a client, or a runner
with no registration:

```baml
class QuotaExhausted {
    account: string,
    implements ai.Failure {
        function retry_safety(self) -> RetrySafety throws never { RetrySafety.Safe }
    }
}
```

Implementing `Failure` is what opts the class into the wrapper
machinery: `Retry` and `Fallback` can classify it, and `retry_if`
judgments receive it. A class without the interface still propagates
normally; the wrappers treat it as unknown and never retry it.

Report `retry_safety()` honestly. A client that cannot know whether
the provider observed the request reports `Unknown`, not `Safe`; an
error thrown after an external side effect reports `Unsafe`.

## The unknown channel

`baml.errors.UnknownError` is the untyped catch-all. A tool or client
that throws something other than a `Failure` has the value wrapped at
the boundary, and fallible signatures follow one convention:

```baml
throws Failure | baml.errors.UnknownError
```

Callers that handle failure match typed arms first and rethrow the
rest (`../02_guides/01_functions/03_calling_functions.md`).
