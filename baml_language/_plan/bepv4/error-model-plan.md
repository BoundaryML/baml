# Plan: the `E | UnknownError` error model in baml_src_temp2

Implements the layered error model (universal wrapper → capability interfaces →
concrete defaults → provider errors → app catch) in the actual corpus.
Supersedes the closed-`ai.Error` proposal in [error-redesign.md](error-redesign.md)
**where they conflict**: the requirement that users throw their own error types
and catch them typesafely rules out a closed kind-enum. The redesign doc's
factual lessons carry over (dead classifiers, exhaustion must preserve the
underlying error, parse failures must carry raw output, effect-safety is
load-bearing).

Everything below marked ✅ was compiled AND executed against the merged branch
(`ddca53ed5`) in scratch probes this session.

## What the compiler supports today (probe results)

✅ Interface-typed unions in `throws` (`throws CallError | baml.errors.UnknownError`)
✅ A user-defined class implementing `ai.Failure` is accepted by that channel statically
✅ App `catch` refines most-specific-first: concrete user class → interface → `UnknownError`,
   and the concrete arm executes correctly through a blind intermediate layer
✅ Interface method dispatch in a catch arm (`c.is_transient()`)
✅ `reassert<T>` / `annotate<T>` semantics (match on generic `T`, wrapped-`T` recovery,
   breadcrumb accumulation, no double-wrap) — with two fixes over the original sketch:
   `annotate` never unwraps (recovery is the handler's job), and the wrapper arm
   precedes the generic arm (else `T = UnknownError` silently skips annotation)
✅ `never` as a generic argument (blind layers call `annotate<never>`)

Blocked / needs filing:

- ❌ `type ExtendUnknownError<E> = E | UnknownError` — generic aliases don't parse
  (issue drafted: parser has no production; 7-error cascade with misleading message)
- ❌ **Non-generic alias as a throws channel breaks catch analysis**: with
  `type FailureChannel = Failure | baml.errors.UnknownError` and `throws FailureChannel`,
  every catch arm is flagged unreachable (E0063) and the catch is treated as
  covering nothing (E0096). Inline unions behave correctly. → FILE THIS.
- ⚠ False `E0063` on a concrete arm (`let q: MyQuotaError`) preceding its
  interface arm (`let c: CallError`) — warns, but the arm demonstrably runs. → FILE.
- ⚠ Generic-arm-shadows-concrete-arm lint (issue already drafted).
- `Self` unresolved in class-method bodies — use the class name.

**Consequence**: until the alias bugs are fixed, the channel is spelled inline:
`throws root.ai.Failure | baml.errors.UnknownError`. That is already the
corpus's existing signature shape — the signatures were right all along; what's
missing is providers actually *throwing classified errors* into them.

## Design

### Layer 1 — `baml.errors.UnknownError` + companions (the universal wrapper)

The stdlib class already exists and the corpus already throws it. Missing are
the channel companions. Userland can't add methods to a stdlib class, so they
land as free functions in `ns_ai/failures/unknown.baml` (eventual home:
methods on `baml.errors.UnknownError` in the stdlib — note in the BEP):

```baml
// RECOVERY — call once, at the handler. Bare or wrapped T is restored;
// wrapper breadcrumbs end their job here. Handlers that want the trail
// destructure: UnknownError { data: let t: T, message: let trail }.
function reassert<T>(data: unknown) -> T | baml.errors.UnknownError throws never {
  match (data) {
    let value: T => value,
    baml.errors.UnknownError { data: let inner: T } => inner,
    let e: baml.errors.UnknownError => e,          // wrapped non-T: never re-wrap
    _ => baml.errors.UnknownError { data: data, message: [] },
  }
}

// PROPAGATION — call while bubbling through a layer. Never unwraps.
// Wrapper arm FIRST (see shadowing footgun). Blind layers pass T = never.
function annotate<T>(data: unknown, message: string) -> T | baml.errors.UnknownError throws never {
  match (data) {
    let e: baml.errors.UnknownError => {
      e.message = e.message.concat([message]);
      e
    },
    let value: T => value,
    _ => baml.errors.UnknownError { data: data, message: [message] },
  }
}
```

Invariants (all runtime-verified): depth never exceeds 1; breadcrumbs
accumulate across layers; known errors keep their identity untouched.
By-convention residue: raw `UnknownError { data: ... }` literals can still
double-wrap — grep gate until field visibility exists.

### Layer 2 — `ns_ai/failures/`: capability interfaces (the static contract)

One independent interface per capability — this is the extension point that
makes user errors first-class. **Methods are trimmed to what generic policy
code actually consumes** (the corpus's `is_resumable`/`is_network_error`/
`is_rate_limit`/`is_parse_error` had zero readers; per the model's own
philosophy, precision comes from catching concrete types, not from interface
getters):

```baml
enum Effects { None, Unknown, Committed }

interface Failure {
  function is_transient(self) -> bool throws never   // consulted by retry/fallback
  function effects(self) -> Effects throws never     // consulted by _may_replay
}
```

**One interface, not four.** The old StreamError/ToolError/RealtimeError split
encoded "which phase failed" — which the caller knows for free (they called
`stream()`), had zero implementers/consumers, and forces cross-cutting errors
like RateLimited to implement every variant. A refinement is added only when
someone writes code that consumes its distinguishing method — the earned
candidates, for later: StreamError.emitted_so_far() (replay after first token
is user-visible even when the call is idempotent), ToolError's
conversation-so-far (loop resumption), RealtimeError.session_alive()
(reconnect vs new session).

Rule (statically enforced by the throws clause, proven in probes): a throw is
legal iff it implements the capability interface or is `UnknownError`.

### Layer 3 — `ns_ai/failures/defaults.baml`: the default error list

**Not a hierarchy — a flat list.** BAML has no class inheritance; interfaces
are the only subsumption and one level is enough. These are the errors we
educate users about; providers throw them unless they have provider-specific
data to add:

```baml
class RateLimited {
  provider: string,
  retry_after_ms: int?,
}
implements Failure for RateLimited {
  function is_transient(self) -> bool throws never { true }
  function effects(self) -> Effects throws never { Effects.None }
}

class NetworkFailure { provider: string, detail: string }       // transient, Effects.Unknown
class Timeout        { provider: string, deadline_ms: int }     // transient, Effects.Unknown
class Refused        { provider: string, reason: string }       // policy refusal; terminal
class InvalidRequest { provider: string, status_code: int?, detail: string }  // terminal
class ParseFailed    { provider: string, raw_output: string }   // terminal; raw text for repair/re-prompt
class Unsupported    { message: string }                        // capability mismatch; terminal
class EffectCommitted { provider: string, operation: string }   // Effects.Committed; never blind-replay
```

(each with its two-method `implements Failure` block — two lines, not the
old five-method quintet.)

Guidance for provider authors, to document: throw a default when it fits;
define your own class implementing `ai.Failure` when you carry
provider-specific data (`OpenAiContentFilter { categories }`); never throw
`UnknownError` directly — it's what the trailing normalize catch produces.

### Layer 4 — provider protocol (`ns_ai/provider/protocol.baml`)

Signatures keep today's shape, now with teeth:

```baml
interface GenerationProvider requires Provider {
  function generate<T>(self, task: Task<T>) -> ResponseWithMetadata<T>
    throws root.ai.Failure | baml.errors.UnknownError
}
```

When the alias-in-throws bug is fixed, this becomes `throws root.ai.FailureChannel`.

### Layer 5 — concrete providers (openai, anthropic, google, claude_code)

Every `throw baml.errors.UnknownError { message: ["openai http ..."] }` site in
`ns_openai/ns_internal/client.baml` (and siblings) becomes:

```baml
if (!http_response.ok()) {
  throw _classify_http("openai", model, http_response);   // shared helper → ai defaults
}
let value = primitive.parse<T>(body) catch (e) {
  _ => throw root.ai.ParseFailed { provider: "openai", raw_output: body },
};
```

`_classify_http` lives once in `ns_ai` (status → RateLimited / InvalidRequest /
NetworkFailure / Refused, parsing retry-after). Pre-send failures
(`build_request`, `specialize_prompt`) and other foreign errors go through the
trailing normalize pattern (rule 3 of the model):

```baml
} catch (e) {
  let known: root.ai.Failure => throw known,
  _ => throw root.ai.annotate<never>(e, "openai generate"),
}
```

### Layer 6 — combinators (`ns_ai/reliability/`)

Same channel, no widening/narrowing (rule 4). `_may_replay` consumes the two
interface methods plus the provider's `ReplayPolicy`:

```baml
function _may_replay(policy: ReplayPolicy, e: root.ai.Failure) -> bool throws never {
  if (!e.is_transient()) { return false; }
  match (e.effects()) {
    Effects.Committed => return false,
    Effects.Unknown => { if (policy.idempotency_key == null) { return false; } },
    Effects.None => {},
  }
  match (policy.kind) {
    ReplayKind.Safe => true,
    ReplayKind.RequiresIdempotencyKey => policy.idempotency_key != null,
    ReplayKind.Never => false,
  }
}
```

Retry/fallback catch shape (both `catch` arms, no `_ => throw e` escape needed
beyond the wrapper arm):

```baml
provider.generate<T>(task...) catch (e) {
  let failure: root.ai.Failure => {
    if (attempt >= self.max_attempts || !_may_replay(policy, failure)) { throw failure; }
    attempt = attempt + 1; continue;
  },
  let u: baml.errors.UnknownError => throw root.ai.annotate<never>(u, "retry(" + self.inner.name() + ")"),
}
```

Exhaustion: rethrow the **last real failure** (typed, so outer layers still see
`RateLimited`), with fallback annotating breadcrumbs on wrapper-typed failures.
`CannotRetry` is deleted; a transient-but-unsafe failure is rethrown as itself
(`is_transient() && effects() != None` tells the caller why replay was refused).
No `"all fallback providers failed"` UnknownError — if no member has the
capability, `throw root.ai.Unsupported { message: "no fallback member can generate" }`.

### Layer 7 — app catch (documented pattern + scenario test)

```baml
p.generate<Invoice>(task) catch (e) {
  let q: MyQuotaError => handle_quota(q.quota_name),                  // concrete, theirs
  let rl: root.ai.RateLimited => backoff(rl.retry_after_ms ?? 1000),  // concrete, ours
  let c: root.ai.Failure => if (c.is_transient()) { retry() } else { fail() },
  let u: baml.errors.UnknownError => report(u.message),
}
```

## Phases

**Phase 0 — probes.** ✅ Done this session (`scratchpad/errproj`); results above.
File the two new compiler issues (alias-in-throws catch analysis; false E0063
on concrete-before-interface arm).

**Phase 1 — `ns_ai/failures/` rewrite.**
New: `unknown.baml` (reassert/annotate), `defaults.baml` (the list),
`Effects` enum; rewrite `protocol.baml` interfaces (trimmed methods).
Delete: the five-boolean quintet, `reliability/errors.baml` (`CannotRetry`),
`unsupported.baml` implements-blocks. Rewrite `_may_replay`.

**Phase 2 — provider protocol.** Signatures unchanged in shape; update doc
comments to state the channel rule and the normalize-catch obligation.

**Phase 3 — concrete providers.** `_classify_http` helper in `ns_ai`; convert
openai first (all ~10 UnknownError sites in `internal/client.baml`), then
anthropic, google, claude_code, transcription/realtime/batch resources.
Grep gate: `throw baml.errors.UnknownError` → zero outside `annotate`.

**Phase 4 — combinators.** Retry/fallback per Layer 6. Preserve existing
scenario behavior in `03_routing_and_reliability`.

**Phase 5 — fakes + tests.** `FakeCallFailure` → throw `ai.RateLimited` /
`ai.Refused` / raw foreign value (exercises normalize). Delete its quintet.
New scenario `03_routing_and_reliability/tests/custom_provider_errors.baml`:
user-defined provider + user-defined error through `fallback(retry(...))`,
caught concretely by the app (the Phase-0 probe, as a native BAML test with
`log.info` instrumentation). Assert breadcrumb accumulation through blind
layers and reassert-recovery at the handler.

**Phase 6 — docs.** `routing-retry-and-fallback.md`: replace the boolean-
classifier story with is_transient/effects + concrete-catch; document the
five rules and the default error list. `tasks-runners-and-results.md`: the
termination-contract rule (declared normal termination → value like
`BudgetReached`; failure to fulfill → error) + the channel for `Runner.run`.
`BudgetReached.reason` → `BudgetReason` enum while touching outcomes.

## Answers to open questions

**Hierarchy or list?** Both layers exist but neither is a class hierarchy:
capability interfaces are the *open static contract* (what a throw must
implement — this is what makes user errors first-class and statically checked),
and the defaults are a *flat concrete list* (what we educate about, what apps
catch by name). BAML has no inheritance; one level of `implements` is the
whole tree, and it's enough.

**Type alias for the channel?** Yes in spirit — blocked twice in practice
(no generic aliases; non-generic alias channels break catch analysis). Spell
the union inline until both are fixed, then mechanically swap to
`ai.FailureChannel`.
