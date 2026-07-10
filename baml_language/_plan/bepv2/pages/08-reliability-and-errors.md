# 8. Reliability and Errors

Retry and fallback are where LLM frameworks quietly do the wrong thing:
re-submitting a billed job, replaying a tool loop's side effects, failing
over a conversation that cannot move. This page defines the two mechanisms
that keep composition honest — a **failure classifier axis** on errors and a
**replay policy** on operations — and the wrappers built on them.

## The failure model: what happened, and did it commit

Every capability error interface requires one shared base. It answers two
orthogonal questions — *what kind of failure was this* and *did the
operation's effect commit* — and deliberately does **not** answer "should
you retry," because no error can know that alone:

```baml
enum baml.errors.FailureKind {
  Transport,        // request may not have arrived; socket dropped
  RateLimit,        // explicit backpressure
  InvalidRequest,   // the request itself is wrong; retrying is futile
  Refusal,          // deliberate decline: model refusal, guardrail, budget policy
  Parse,            // the response arrived but did not match the schema
  Unsupported,      // the backend cannot do this operation at all
  Cancelled,        // caller- or server-initiated cancellation
}

enum baml.errors.CommitState {
  NotCommitted,     // known: no effect happened (request never left, 429 at the door)
  Committed,        // known: the effect happened (job billed, message appended)
  Unknown,          // the dangerous default: a 500 after the server may have acted
}

interface baml.errors.Failure {
  function kind(self) -> FailureKind throws never
  function commit_state(self) -> CommitState throws never
  function retry_after(self) -> baml.time.Duration? throws never   // default null
  function is_resumable(self) -> bool throws never                 // default false
}

interface baml.errors.CallError requires baml.errors.Failure {}
interface baml.errors.StreamError requires baml.errors.Failure {}
interface baml.errors.BackgroundError requires baml.errors.Failure {
  function is_terminal(self) -> bool throws never   // server killed the job vs poll hiccup
}
```

Why two axes instead of one `is_retryable` bool on the error: the same
failure can be safe or unsafe to replay depending on *what was being
attempted*. A `Transport`/`Unknown` error during a read-only generation is
worth retrying; the identical error after a tool executed a payment is not.
The error reports facts it can know (`kind`, `commit_state`); the retry
decision belongs to the layer that also knows the operation (below).

Because the base is `require`d, no capability's error channel can drop it —
any `catch` can triage without knowing the concrete class:

```baml
let r = ExtractInvoice(doc) catch (e) {
  let f: baml.errors.Failure => match (f.kind()) {
    Refusal   => escalate_to_human(doc),
    RateLimit => retry_later(doc, f.retry_after()),
    _         => throw e,
  },
  _ => throw e,   // UnknownError: no classification, no assumptions
}
```

Truthful classification is a hard rule for error authors. A budget stop is
`kind: Refusal, commit_state: NotCommitted` — never a fake `Transport`.

## Replay policy: the operation-level half

Some operations must not be re-driven even when the error looks safe (a
background submit that already billed) and some never (a live session that
already played audio). That is a property of the **operation**, not the
provider — one provider can offer safe reads, keyed submits, and
never-replay live sessions simultaneously. Every driver constructs its
operation with a policy:

```baml
enum ReplayKind {
  Safe,                     // idempotent by nature: re-drive freely
  RequiresIdempotencyKey,   // re-drive only with the same key
  Never,                    // no automatic replay, ever
}

class ReplayPolicy {
  kind: ReplayKind,
  idempotency_key: string?,
}
```

The retry decision is one pure stdlib function combining both halves — this
is the **only** place "may I replay" is computed, and wrappers call it
instead of improvising:

```baml
function may_replay(policy: ReplayPolicy, failure: baml.errors.Failure) -> bool {
  // futile regardless of safety:
  match (failure.kind()) {
    InvalidRequest => { return false; },
    Refusal        => { return false; },
    Unsupported    => { return false; },
    Cancelled      => { return false; },
    _ => {},
  }
  // safe only if the operation tolerates a duplicate of a maybe-committed effect:
  match (policy.kind) {
    Safe                   => true,                          // idempotent: even Committed is fine
    RequiresIdempotencyKey => policy.idempotency_key != null // keyed: duplicate collapses server-side
                              && failure.commit_state() != CommitState.Committed,
    Never                  => false,
  }
}
```

Two corollaries with teeth:

- `.background` without an idempotency key still runs — but a *retry
  wrapper around it* refuses with a typed error, because
  `RequiresIdempotencyKey` is unmet. The key is what makes "submit again"
  mean "the same job."
- Local post-processing (metadata projection, your parse callbacks) runs
  *outside* the replay scope. A bug in your projection throws your bug; it
  never re-sends the prompt to a second provider.

## Retry

```baml
let Reliable = Fast.with_retry(baml.ai.RetryPolicy {
  max_attempts: 3,
  base_delay: baml.time.Duration.from_milliseconds(200),
})

let invoice = ExtractInvoice(doc, client = Reliable)
```

`with_retry` returns a wrapper holding the inner provider. The wrapper
claims the standard capabilities and forwards each with its own rules:

- **generate**: the retry loop consults `may_replay(policy, failure)` per
  error — a rate limit (`NotCommitted`) re-drives with backoff; a 500 with
  `Unknown` commit state on an unkeyed operation surfaces immediately.
- **stream**: retries *initiation only*; after the first observable chunk,
  failures surface (the consumer has seen data; silent restart would lie).
- **background**: forwards only with an idempotency key; refuses otherwise
  with typed `CannotRetry`.
- **realtime / sessions**: never auto-replayed; the wrapper's methods refuse
  with `Unsupported`/`CannotRetry` rather than pretending.

### Why the wrapper holds an existential

The wrapper stores the inner provider as the *existential* `Provider` and
re-discovers capabilities by `match` inside each forwarded method:

```baml
class Retry {
  inner: baml.ai.Provider,          // existential — the inner's concrete type is erased
  policy: RetryPolicy,

  implements baml.ai.Provider {}

  implements baml.ai.Generate {
    function generate<T>(self, request: baml.ai.Request<T>) -> baml.ai.Response<T>
        throws baml.errors.CallError | baml.errors.UnknownError {
      let g: baml.ai.Generate = match (self.inner) {
        let g: baml.ai.Generate => g,
        _ => throw baml.errors.Unsupported { message: "inner cannot generate" },
      };
      let replay = baml.ai.ReplayPolicy { kind: baml.ai.ReplayKind.Safe, idempotency_key: null };
      let attempt = 0;
      while (true) {
        let r = g.generate<T>(request.for_provider(self.inner)) catch (e) {
          let f: baml.errors.Failure => {
            attempt = attempt + 1;
            if (attempt >= self.policy.max_attempts || !baml.ai.may_replay(replay, f)) {
              throw e;                        // futile, unsafe, or out of budget: surface it
            }
            baml.sys.sleep(self.policy.backoff(attempt, f.retry_after()));
            continue;                         // kind + commit state + policy all permit
          },
          _ => throw e,
        };
        return r;
      }
    }
  }

  implements baml.ai.Streaming {
    // forwards the same way: match inner for Streaming, retry INITIATION only
  }
}
```

Refusal happens at call time (`Unsupported` when the stage runs). The
alternative looks better and is worse — a per-capability wrapper that
refuses at *compile* time:

```baml
class RetryGenerate {
  inner: baml.ai.Generate,           // narrow: statically checked...
  policy: RetryPolicy,
  implements baml.ai.Provider {}
  implements baml.ai.Generate { ... }
}

let m = baml.ai.OpenAi { ...Fast }             // OpenAi is Generate AND Streaming AND Tools
let wrapped = RetryGenerate { inner: m, policy: p }

let invoice = ExtractInvoice(doc, client = wrapped)          // fine
let stream  = ExtractInvoice.stream(doc, client = wrapped)   // Unsupported!
```

The narrow field *discarded* the inner's sibling capabilities: `wrapped` is
only `Generate`, so streaming through it fails even though the inner streams
fine. Preserving the inner's full surface requires claiming it and checking
at runtime — which is exactly what the existential wrapper does. The
capability-preserving *and* compile-time-checked version would need
`with_retry(self) -> typeof(self)` — Self/intersection types, a type-system
future this design is forward-compatible with. Until then: existential
wrapper, per-operation refusal, typed errors.

## Fallback

```baml
let Resilient = Fast.fallback_to(Careful)

let invoice = ExtractInvoice(doc, client = Resilient)
```

Rules that make fallback safe rather than merely convenient:

- Members are tried per classified error and replay policy — a policy
  refusal does **not** fail over (the second model would happily produce
  what the first was told not to), an unknown-commit-state error does not
  re-drive.
- Each member gets the request rebound via `request.for_provider(member)`,
  so provider-sensitive prompt context re-renders per attempt.
- Streaming fallback happens only before the first observable chunk.
- Stateful capabilities do not fail over mid-flight: a session is bound to
  the member that opened it. "Fallback" for stateful work means *pick a
  capable member up front*.

## Observability

```baml
let meter = baml.ai.UsageMeter {}
let invoice = ExtractInvoice(doc, client = Fast.traced(meter))
log.info(`attempts: ${meter.calls()}, tokens: ${meter.total().input_tokens}`)
```

Tracing is a wrapper too, with one non-negotiable: it records every
*attempt*, including failed ones — reconstructing attempt history from the
winning response is structurally impossible once retries exist. When a
traced provider returns a resource (page 6), the resource is wrapped so
polls and session turns remain visible.

## Fluent sugar and out-of-body `implements`

The wrapper value is the semantics; dot syntax is convenience. `Provider`
remains an empty marker, and the canonical composition operations are the free
functions `retry`, `fallback`, and `traced` from page 9. The standard library
adds their fluent spellings through a syntax-only extension interface:

```baml
interface ProviderFluent requires Provider {
  function with_retry(self, policy: RetryPolicy) -> Retry {
    Retry { inner: self, policy: policy }
  }

  function fallback_to(self, other: Provider) -> Fallback {
    Fallback { members: [self, other] }
  }

  function traced(self, meter: UsageMeter) -> Traced {
    Traced { inner: self, meter: meter }
  }
}

implements<T extends Provider> ProviderFluent for T {}
```

An out-of-body blanket implementation is important here. It gives the sugar
to every concrete `T` that implements `Provider`, including a provider declared
by an application and a wrapper such as `Retry`, without adding methods to each
class declaration. `ProviderFluent` is not a capability: code MUST NOT match on
it to discover an interaction shape, and wrappers MUST NOT use it as evidence
that generation, streaming, sessions, or any other operation is supported.

Blanket implementations apply to concrete receivers, not interface
existentials. Consequently these two layers are intentionally different:

```baml
// Concrete receiver: the blanket implementation supplies dot syntax.
let reliable = baml.ai.OpenAi { ... }.with_retry(policy)

// Erased receiver: use the canonical negotiation/composition function.
let selected: baml.ai.Provider = route_for(tenant)
let reliable = baml.ai.retry(selected, policy)
```

`implements ProviderFluent for Provider {}` is not an escape hatch: interfaces
are not concrete implementation targets. Making `Provider` require
`ProviderFluent` is also the wrong direction. It would turn optional syntax
into part of the marker's semantic contract, require every provider to carry
that implementation explicitly, and cannot also make `ProviderFluent` require
`Provider` without a cyclic `requires` chain.

### Library-owned sugar

The same mechanism is available to libraries, but opinionated policies do not
automatically enter the standard fluent surface. A judge-gated escalation
library can define its own wrapper, canonical constructor, and extension:

```baml
class JudgePolicy {
  rubric: string,
  min_score: float,
}

class JudgeGated {
  cheap: Provider,
  judge: Provider,
  strong: Provider,
  policy: JudgePolicy,
  implements Provider {}
  implements Generate { ... }
}

function judge_gated(
  cheap: Provider,
  judge: Provider,
  strong: Provider,
  policy: JudgePolicy,
) -> JudgeGated {
  JudgeGated { cheap: cheap, judge: judge, strong: strong, policy: policy }
}

interface JudgeGateFluent requires Provider {
  function judged_by(
    self,
    judge: Provider,
    strong: Provider,
    policy: JudgePolicy,
  ) -> JudgeGated {
    judge_gated(self, judge, strong, policy)
  }
}

implements<T extends Provider> JudgeGateFluent for T {}
```

The return type is the concrete `JudgeGated` wrapper, so other blanket fluent
interfaces apply and chaining remains available:

```baml
let model = Cheap { ... }
  .judged_by(Judge { ... }, Strong { ... }, policy)
  .with_retry(retry_policy)
```

The core/library/application boundary is:

- stable, provider-independent composition policy (`retry`, `fallback`,
  `traced`) may have standard fluent sugar;
- reusable but opinionated policy (`judged_by`, semantic cache, calibrated
  escalation) belongs to the library that defines its wrapper and policy;
- a decision with business meaning (tenant routing, legal-review escalation,
  account tier) is an ordinary function or workflow, not a provider method.

Sugar MUST be a thin delegate to the same constructor or implementation used
by the free-function form. It MUST NOT introduce a second policy engine whose
retry, error, accounting, or capability-forwarding behavior can drift.

## Routing is not a combinator

Business routing is ordinary code returning a provider (page 4):

```baml
let invoice = ExtractInvoice(doc, client = route_for(tenant))
```

Reserve wrappers for generic policies (retry, fallback, trace, balance);
use functions for decisions with business meaning. Both compose:
`baml.ai.retry(route_for(tenant), policy)`.

## Alternatives considered

**Provider-wide effectfulness** (one `is_effectful()` on the provider gates
all re-drive). Rejected: it is one bit for a provider that may expose safe
reads *and* billed submits *and* live sessions; the bit is either too strict
(nothing retries) or too loose (submits replay). Replay policy attaches to
the operation, where the truth lives.

**Only the transport trio on errors** (`is_network_error` / `is_rate_limit`
/ `is_parse_error`). Rejected: every new capability either redefines them,
drops them, or fake-implements them with vacuous `false`s; none answers "is
re-driving safe," which is the only question a combinator has.

**Per-capability wrapper classes as the public surface**
(`RetryGenerate`, `RetryStreaming`, ...). Rejected as primary for the
narrowing problem above; available as a pattern when narrowing is the
intent (a function that should only ever hand out a retried `Generate`).

**Retry as declarative config only** (a `retry_policy` block on the client
declaration). Insufficient alone: policies need to differ per call site and
compose with routing and wrapping. Declarative sugar can lower to
`with_retry`; the value form is the semantics.

**Catching and classifying in user code** (no wrappers; everyone writes
retry loops). The classifier axis makes this *possible* — it is not the
recommendation. Hand-rolled loops forget backoff, forget commit state, and
forget streams; the wrappers encode the rules once.
