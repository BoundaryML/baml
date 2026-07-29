# Errors and error handling

Every fallible AI operation throws on one channel: a classified failure that
implements `ai.Failure`, or the universal wrapper `baml.errors.UnknownError`.
Errors carry facts about what happened; retry, fallback, and application code
decide what to do with them.

## Utilities used

| Utility | What it does |
| --- | --- |
| `ai.Failure` | The classification interface every AI error implements |
| `ai.RateLimited`, `ai.ParseFailed`, ... | Default errors providers throw |
| `ai.Effects` | Whether a failed attempt may have committed side effects |
| `baml.errors.UnknownError` | Universal wrapper for anything unclassified |
| `UnknownError.with_message` | Annotates an error as it bubbles through a layer |
| `UnknownError.from` | Recovers a typed error at the handler |

## The channel

```baml
interface Failure {
  function is_transient(self) -> bool throws never
  function effects(self) -> Effects throws never
}
```

Every provider capability declares the same channel — `throws ai.Failure |
baml.errors.UnknownError` — and so does every combinator that wraps one.
A throw is legal iff the value implements `ai.Failure` or is routed through
`baml.errors.UnknownError`, and the compiler enforces this at the `throws`
clause. Retry and fallback never learn concrete error types: they consult
`is_transient()` and `effects()` and nothing else. Precision belongs to the
catch site.

The two methods are deliberately all there is. `is_transient` answers "could
an identical attempt plausibly succeed?" — rate limits and transport blips
yes; refusals, invalid requests, and parse failures no. `effects` answers
"what may have committed before the failure?" — see
[Be careful after side effects](routing-retry-and-fallback.md).

## The default vocabulary

Providers throw these unless they have provider-specific data to carry:

| Error | Thrown when | Transient |
| --- | --- | --- |
| `RateLimited { provider, retry_after_ms }` | The provider said slow down | yes |
| `NetworkFailure { provider, detail }` | Transport failed; outcome unobserved | yes |
| `InvalidRequest { provider, status_code, detail }` | The request itself was rejected | no |
| `Refused { provider, reason }` | A safety filter or policy block | no |
| `ParseFailed { provider, raw_output }` | The response is not the requested type | no |
| `EffectCommitted { provider, operation }` | The attempt failed after committing | no |

`ParseFailed` carries the raw model text on purpose: the useful responses to a
parse failure — log it, repair it, re-prompt with it — all need the text.
Blind re-send of the same prompt is an application decision, never a default.

## Example

Catch most-specific first: concrete types for precision, the interface for
triage, the wrapper as the escape hatch. The example uses the shared
support-ticket fixtures: `ResolveTicket`, `sample_ticket()`, and the
`careful_model()` escalation target.

```baml
let ticket = sample_ticket();
let resolution = ResolveTicket(ticket) catch (e) {
  let limited: ai.RateLimited => {
    let wait = baml.time.Duration.from_milliseconds(limited.retry_after_ms ?? 1000);
    baml.sys.sleep(wait) catch (e) { _ => null };
    ResolveTicket(ticket, $provider = careful_model())
  },
  let parse: ai.ParseFailed => {
    log.warn("unparsed model output: " + parse.raw_output);
    throw parse;
  },
  let failure: ai.Failure => {
    if (failure.is_transient()) {
      ResolveTicket(ticket, $provider = careful_model())
    } else {
      throw failure;
    }
  },
  let unknown: baml.errors.UnknownError => {
    log.warn("unclassified failure: " + unknown.message.join(" <- "));
    throw unknown;
  },
}
```

### Illustrative output

```console
[INFO] provider attempt: openai (fast_model)
[WARN] openai returned RateLimited { retry_after_ms: 250 }
[INFO] backing off 250ms, retrying on anthropic (careful_model)
[INFO] Resolution { category: "billing", priority: Urgent, ... }
```

The first two arms are runtime refinements: the static error type is only
`ai.Failure | baml.errors.UnknownError`, and the same catch works whether the
handle is a concrete provider or an `ai.CompletionProvider`.

## Defining your own errors

A provider — yours or a vendor package's — throws its own error by
implementing the interface. Two methods, no registration; here the error
itself carries its classification:

```baml
class VendorQuotaExceeded {
  quota_name: string,
  transient: bool,
}

implements ai.Failure for VendorQuotaExceeded {
  function is_transient(self) -> bool throws never { self.transient }
  function effects(self) -> ai.Effects throws never { ai.Effects.None }
}
```

The `ai` namespace never learns this type exists. It rides the channel because
it implements `Failure`; retry and fallback classify it through the interface
and rethrow it unchanged; the application that knows the type catches it
concretely. The reliability scenarios throw it from `QuotaLimitedProvider`, a
provider wrapper that fails with this error until its budget is spent:

```baml
let limited = QuotaLimitedProvider {
  inner: fake_resolution(), failures_remaining: 99, transient: false, calls: 0,
};

let resolution = ResolveTicket(sample_ticket(), $provider = ai.retry(limited, 3)) catch (e) {
  let quota: VendorQuotaExceeded => {
    log.warn("quota " + quota.quota_name + " exhausted; escalating");
    ResolveTicket(sample_ticket(), $provider = careful_model())
  },
  let failure: ai.Failure => throw failure,
  let unknown: baml.errors.UnknownError => throw unknown,
}
```

Because the thrown error says `transient: false`, `ai.retry` refuses to
replay it: the catch sees the concrete type after exactly one attempt.

## Unknown errors: wrap, annotate, recover

Anything foreign — an HTTP client's exception, a JSON parser's error, a
subprocess exit — is normalized at the boundary where it appears, with a
trailing catch:

```baml
} catch (e) {
  let known: ai.Failure => throw known,
  _ => throw baml.errors.UnknownError.with_message<never>(e, "openai generate"),
}
```

`with_message<T>` never unwraps: a known `T` passes through untouched, an
existing wrapper gains one breadcrumb, anything else is wrapped fresh. The
type argument names the errors this layer's channel already knows; a layer
with no typed channel of its own passes `never`. Because annotation preserves
the wrapper, context accumulates as the error bubbles:

```console
UnknownError { data: <socket reset>, message: ["transport", "retry(openai)"] }
```

At the handler — and only there — `UnknownError.from<T>` reasserts the
channel: a bare or wrapped `T` is restored to its identity, an unrelated
wrapper passes through, and nothing ever wraps twice.

```baml
match (baml.errors.UnknownError.from<VendorQuotaExceeded>(e)) {
  let quota: VendorQuotaExceeded => log.warn("quota " + quota.quota_name + " exhausted"),
  let unknown: baml.errors.UnknownError => throw unknown,
}
```

Never throw `baml.errors.UnknownError` by constructing it directly; the
normalize catch is what maintains the invariant that wrappers do not nest.

## Exhaustion keeps the real error

When retry gives up or every fallback member has failed, the last real
failure is rethrown — classification intact — rather than being flattened
into a summary error. An outer layer wrapping `ai.fallback([ai.retry(a, 3), b])`
still sees `ai.RateLimited` and can act on it. A failure that is transient but not
replay-safe (`effects()` is `Committed`, or `Unknown` without an idempotency
key) is likewise rethrown as itself: its own facts say why replay was
refused.

## Errors are for faults

A declared, normal termination of a runner's state machine is a value, not an
error: `Done`, `BudgetReached`, and `Handoff` are members of
[the agent outcome union](approvals-limits-and-handoffs.md). Failure to
fulfill the contract — a 429 after retries, a parse failure, a dead stream —
is an error on the channel. If the caller asked for the determination, return
it; if the caller's question went unanswered, throw.
