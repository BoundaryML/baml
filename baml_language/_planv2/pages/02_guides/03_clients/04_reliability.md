# Reliability

## The error model

Errors carry facts; callers make judgments. A failure class states
what happened (`RateLimited`, `NetworkFailure`) and reports the one
fact only the failing layer can know — whether replaying the failed
call is safe:

```baml
enum RetrySafety {
    Safe,        // failed before anything could commit
    Unknown,     // request sent, outcome never observed
    Unsafe,      // effects happened; never blind-replay
}

interface Failure {
    function retry_safety(self) -> RetrySafety throws never
}
```

Whether a failure is worth retrying is never a self-report. It is
expressed by the caller: by catching concrete classes, or by giving
the `Retry` wrapper a predicate over failures (`retry_if`, shown
below).

## The error catalog

The classified vocabulary, in brief; the full table with fields and
conditions is `../../04_reference/03_errors.md`:

| Class | Meaning |
|---|---|
| `RateLimited` | the provider throttled the request; may carry `retry_after_ms` |
| `NetworkFailure` | transport failed or the response never arrived intact |
| `InvalidRequest` | the provider rejected the request as malformed or unauthorized |
| `Refused` | the model or provider declined to answer |
| `ParseFailed` | the response arrived but could not be used |

All report `Safe`. The turn protocol is what makes that true:
a failed `invoke` returns no turn, and the runner commits nothing, so
nothing local advanced.

## Retry

`Retry` is a client that wraps a client:

```baml
let reliable: Client = ai.clients.Retry {
    inner: ai.clients.resolve("openai/gpt-5.6"),
    max_attempts: 3,
    backoff: Backoff { initial_ms: 250, multiplier: 2, max_ms: 10000 },
};
let trip: Itinerary = PlanTrip("2 weeks in Japan", $client = reliable);
```

`Retry.invoke` calls the inner client and retries a thrown failure
when two conditions hold: the failure reports `Safe`, and the
judgment accepts it. The default judgment declines `Refused`,
`InvalidRequest`, and `ParseFailed` — a request the provider rejected
does not become valid by resending — and retries the rest. A
`RateLimited.retry_after_ms` hint overrides the computed backoff.

The `retry_if` field replaces the default judgment with your own
predicate. It receives the classified failure and answers whether to
resend; the `RetrySafety` gate stays in front of it and is not
overridable:

```baml
let careful: Client = ai.clients.Retry {
    inner: ai.clients.resolve("openai/gpt-5.6"),
    max_attempts: 5,
    retry_if: (f: Failure) -> bool {
        match (f) {
            let r: RateLimited => true,   // only throttles are worth waiting out
            _ => false,
        }
    },
};
```

Retry wraps single turns. Whole-run retries belong to a wrapping
runner (`../02_specs_and_runners/03_writing_a_runner.md`), and the two
compose without overlap because they act at different boundaries.

## Fallback

`Fallback` tries each member in order:

```baml
let resilient: Client = ai.clients.Fallback {
    members: [
        ai.clients.resolve("openai/gpt-5.6"),
        ai.clients.resolve("anthropic/claude-sonnet-5"),
    ],
};
```

On a thrown failure that reports `Safe`, `invoke` advances to the next
member and re-invokes. Every `invoke` renders from scratch, so
advancing needs no conversion step; the next member renders the same
journal in its own format. When the last member fails, the last
failure propagates. Wrappers nest: a `Fallback` of `Retry`-wrapped
members retries each provider before advancing.

## HTTP classification

`classify_http(provider, status_code, body)` maps a non-2xx response
to a failure class. `wire.send_as` applies it, so clients built on
the wire library classify uniformly:

| Status | Class |
|---|---|
| 429 | `RateLimited` |
| 408, 5xx | `NetworkFailure` |
| other 4xx | `InvalidRequest` |

A client with better information — a provider error body that
distinguishes a content refusal from a malformed request — classifies
more precisely before falling back to the table.

## Reading the provider's error response

Every classified wire failure carries the response body it was
classified from: `raw_body` on `RateLimited`, `NetworkFailure`,
`InvalidRequest`, and `Refused`, and `raw_output` on `ParseFailed`.
`InvalidRequest` also carries the status code. Catch the class and
read it:

```baml
let trip: Itinerary = PlanTrip(request) catch_all (e) {
    let i: InvalidRequest => {
        log.error(`provider rejected (${i.status_code ?? 0}): ${i.raw_body ?? i.detail}`);
        throw e
    },
    _ => throw e,
};
```

The body is the limit of what an error carries. Response headers and
the full envelope are not retained: the journal records no HTTP
traffic (`../04_the_journal.md`), and `wire.send_as` classifies before
returning, so its caller never holds the `Response`. A client that
needs headers — a provider request id for a support ticket — calls
`baml.http.send` directly and classifies with `classify_http` itself.
