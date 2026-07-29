# Routing, retry, and fallback

Routing chooses a provider before work starts. Retry repeats a safe failure.
Fallback moves to another compatible provider after a failure. All three are
provider decisions, so all three compose as providers: `ai.retry(...)` and
`ai.fallback(...)` wrap providers and are themselves providers.

## Utilities used

| Utility | What it does |
| --- | --- |
| `task.with_provider(...)` | Routes one task before execution |
| `ai.retry(provider, max_attempts)` | A provider that repeats its inner provider when replay is safe |
| `ai.fallback([providers])` | A provider that tries compatible members in order |
| `ai.ReplayPolicy` | The inner provider's contract for whether an operation may repeat |

## Example

The example uses the shared support-ticket models (`SupportTicket`,
`Resolution`, `sample_ticket()`) and the shared provider values
`fast_model()` and `careful_model()`.

```baml
function ResolveTicket(ticket: SupportTicket) -> Resolution {
  provider: fast_model()
  prompt: `
    Resolve this support ticket.
    Subject: ${ticket.subject}
    Body: ${ticket.body}

    ${ctx.output_format}
  `
}

let resolution = ResolveTicket@task(sample_ticket())
  .with_provider(ai.fallback([
    ai.retry(fast_model(), 3),
    careful_model(),
  ]))
  .run(runner = ai.run.Completion<Resolution>.new())
```

### Illustrative output

```console
[INFO] provider attempt: openai (fast_model)
[WARN] openai returned RateLimited { retry_after_ms: 250 }
[INFO] retrying openai: attempt 2 of 3
[WARN] openai attempts exhausted
[INFO] falling back to anthropic (careful_model)
[INFO] returned Resolution { category: "billing", ... }
```

The wrappers are providers, so the runner and result type do not change: this
expression still returns `Resolution`. Before each attempt the task is rebound
and re-rendered for the provider actually being tried, and composition nests
freely — `ai.fallback([ai.retry(a, 3), b])` retries `a` before moving to `b`,
while an outer catch still sees the real classified failure (see
[Errors and error handling](errors-and-error-handling.md)).

Fallback is ordered recovery, not load balancing. It continues only after a
failure whose own classification (`is_transient()`, `effects()`) and the
member's `ai.ReplayPolicy` say replay is safe.
Its provider list must contain at least one member; an empty fallback is
rejected with `baml.errors.InvalidArgument`. On exhaustion, the last member's real
failure is rethrown, classification intact.

## Route before running

Use ordinary application code when tenant policy, data residency, cost, or
request type can choose the provider up front:

```baml
function route_ticket(ticket: SupportTicket) -> ai.CompletionProvider {
  if (ticket.customer_tier == "pro") {
    careful_model()
  } else {
    fast_model()
  }
}

let ticket = sample_ticket();
let routed = ResolveTicket@task(ticket)
  .with_provider(route_ticket(ticket));

let resolution = routed.run(
  runner = ai.run.Completion<Resolution>.new(),
)
```

### Illustrative output

```console
[INFO] route_ticket matched policy: customer_tier = "pro"
[INFO] rebound task to anthropic (careful_model)
[INFO] rendered provider-specific request
[INFO] Completion returned Resolution
```

The router returns the capability required by the runner. An incompatible
provider is therefore a type error instead of a failed request.

## Be careful after side effects

Retrying a read is different from retrying a refund. After an application tool
succeeds, a later model failure does not make the entire Agent run replay-safe.
A provider states its own contract through `replay_policy`: `ai.ReplayKind.Safe`
permits replay, `ai.ReplayKind.RequiresIdempotencyKey` permits it only with a
key, and `ai.ReplayKind.Never` refuses it. A provider that commits remote effects
declares that explicitly:

```baml
class EffectfulProvider {
  inner: ai.testing.FakeProvider,
  implements ai.Provider {
    function name(self) -> string throws never { "effectful" }
    function render_shorthand(self) -> string throws never { self.inner.render_shorthand() }
  }
  implements ai.CompletionProvider {
    function complete<T>(self, task: ai.Task<T>) -> ai.ResponseWithMetadata<T>
        throws ai.Failure | baml.errors.UnknownError {
      self.inner.complete<T>(task.with_provider(self.inner))
    }
    function replay_policy<T>(self, task: ai.Task<T>) -> ai.ReplayPolicy throws never {
      ai.ReplayPolicy { kind: ai.ReplayKind.Never, idempotency_key: null }
    }
  }
}
```

When policy cannot prove safety, retry rethrows the failure as itself: its
classification (`is_transient()`, `effects()`) already says why replay was
refused. See [Errors and error handling](errors-and-error-handling.md) for the
full error model.
