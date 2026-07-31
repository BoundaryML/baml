# Errors and error handling

BAML agents report problems on two channels, and which channel a problem
takes is decided by **what survives**, not by what went wrong:

- **Values.** A run that stops at a committed, resumable state returns an
  *outcome* — `Stopped`, `Handoff`, `Interrupted`, or `Failed` (a
  classified fault that struck *after* committed progress, carried inside
  the value because the value is what you need to continue). You hold
  something worth continuing, so you match on it.
- **Throws.** A call that leaves you nothing to hold — a fault before any
  progress, a capability gap, a demand for completion that wasn't met —
  throws. The invariant: **an Agent run returns an outcome at a committed
  state, or throws having changed nothing.**

Tool failures are on neither channel: a failed tool becomes a `ToolError`
value in the conversation, and the *model* recovers. You never catch one.

This page shows what each call can throw and what to do about it. For the
outcome union itself, see
[Agent sessions](agent-sessions.md) and
[Approvals, limits, and handoffs](approvals-limits-and-handoffs.md).

## Utilities used

| Utility | What it does |
| --- | --- |
| `ai.Failure` | The interface every AI error implements — it reports facts, not judgments |
| `ai.RateLimited`, `ai.ParseFailed`, ... | Default errors providers throw |
| `ai.IncompleteRun` | Thrown by `complete()` — carries the stopped outcome; its own `throws` term, NOT an `ai.Failure` |
| `ai.run.SessionMismatch` | A session token paired with the wrong task |
| `baml.errors.Unsupported` | The provider lacks a capability |
| `baml.errors.UnknownError` | Universal wrapper for anything unclassified |
| `ai.Effects` | Whether a failed attempt may have committed side effects |

## What each call throws

| Call | Returns | Throws |
| --- | --- | --- |
| `task.run(runner)` | five-outcome union (incl. `Failed`) | `ai.Failure`/`UnknownError` *before progress only*, `Unsupported` |
| `task.complete(runner?)` | `T` | the above, plus `ai.IncompleteRun` on a stop; a `Failed` run rethrows its cause |
| `session.send(msg)` / `resume()` / `submit_tool_results(results)` | five-outcome union | pre-progress `ai.Failure`/`UnknownError`, `Unsupported`, `ai.run.SessionBusy`, guard `InvalidRequest`s |
| `session.complete(msg)` | `T` | the above, plus `ai.IncompleteRun` |
| `AgentSession.start(task)` | session | `Unsupported` if the provider cannot run an agent, `ai.Failure`, `UnknownError` |
| `AgentSession.of(task, outcome)` / `from(task, conversation)` | session | `ai.InvalidRequest` if the conversation's output type is not `T` |
| `AgentSession.from(task, messages)` | session | `Unsupported` if the provider cannot import, `ai.Failure`, `UnknownError` |
| `session.save()` | token | `Unsupported` if the provider cannot serialize, `UnknownError` |
| `AgentSession.restore(task, token)` | session | `ai.run.SessionMismatch` (identity or contract), `Unsupported`, `UnknownError` |
| `session.fork()` | session | `Unsupported` if the provider cannot save/restore, `UnknownError` |
| `session.move_to(provider)` | session | `ai.InvalidRequest` on unanswered tool calls, `Unsupported` if the destination cannot import, `UnknownError` |

## The failure vocabulary

Providers throw these unless they have provider-specific data to carry.
Errors report **facts** (what happened, what may have committed); whether a
failure is worth retrying is **your judgment**, made by catching concrete
types or by passing `retry_if` to `ai.retry`. The last column is guidance,
not API:

| Error | Thrown when | Retry usually helps? |
| --- | --- | --- |
| `RateLimited { provider, retry_after_ms }` | The provider said slow down | yes, after backing off (`retry_after_ms` overrides a computed backoff) |
| `NetworkFailure { provider, detail }` | Transport failed; outcome unobserved | yes for a model step — `Effects.None`, because a failing step leaves its conversation unchanged; the transactional contract is what makes replay safe |
| `InvalidRequest { provider, status_code, detail }` | The request itself was rejected | no — fix the request |
| `Refused { provider, reason }` | A safety filter or policy block | no — identical replay, identical refusal |
| `ParseFailed { provider, raw_output }` | The response is not the requested type | not blindly — repair or re-prompt with the text |
| `EffectCommitted { provider, operation }` | A non-conversation operation failed after committing | never blind-replay |
| `IncompleteRun { outcome }` | You demanded completion; the run stopped instead | no — resume the carried outcome (not an `ai.Failure`; catch it by name) |

This table is also `ai.retry`'s default judgment: with no `retry_if`, retry
declines `Refused`, `InvalidRequest`, and `ParseFailed` and replays the
other effect-safe failures — including `NetworkFailure`.

`ParseFailed` carries the raw model text on purpose: the useful responses —
log it, repair it, re-prompt with it — all need the text.

## Handle stops as values when stopping is routine

An interactive application treats policy stops, interruptions, and handoffs
as normal control flow. Match; don't catch:

```baml
match (session.send(msg)) {
    let done: ai.Done<string> => show(done.value),
    let stopped: ai.Stopped => show(`paused after ${stopped.steps_taken} steps (${stopped.reason})`),
    let handoff: ai.Handoff => run_and_answer(session, handoff.call),
    let interrupted: ai.Interrupted => show("stopped — send a message or resume"),
    let failed: ai.Failed => show(`fault at a committed checkpoint: ${failed.cause} — resumable`),
}
// In every arm, the session has already advanced to the committed state.
```

## Demand completion and handle `IncompleteRun`

A script or pipeline that only wants the `T` calls `complete`. If the run
stops instead, `complete` throws `ai.IncompleteRun` — and the conversion is
lossless: the error carries the actual outcome, and the outcome carries the
committed conversation. A demanded completion never destroys the partial
run; it only routes it to `catch` instead of `match`.

`IncompleteRun` does NOT implement `ai.Failure`. A demanded completion that
stopped at a resumable boundary is control flow, not a fault, so it is its
own term in the `throws` union — `throws ai.IncompleteRun | ai.Failure |
baml.errors.UnknownError` — and a generic `ai.Failure` catch arm never
absorbs it. Catch it by name, and decide what a stop means for your call
site.

```baml
let task = ResolveTicket@task(sample_ticket());
let plan = task.complete() catch (e) {
    let incomplete: ai.IncompleteRun => {
        //# The partial run is inside the error — pick it up mid-stride
        let session = ai.run.AgentSession<Resolution>.of(task, incomplete.outcome);
        match (incomplete.outcome) {
            let handoff: ai.Handoff => { /* session.submit_tool_results(...) */ },
            _ => { /* session.resume(runner = ai.run.Agent<Resolution>.new(max_steps = 64)) */ },
        };
        fallback_plan()
    },
}
```

`session.complete(msg)` behaves the same way with one extra guarantee: the
session has *already advanced* to the committed checkpoint when the throw
reaches you, so `resume` on the same session continues correctly.

> **Note:** act on the error before continuing the session. The
> conversation inside `IncompleteRun` is the live object, not a snapshot —
> if you continue the session first, the error's view advances with it.
> `fork()` before recovery if you need the stop-state preserved.

## Recover with `resume`, never by resending

After any mid-turn fault — a `Failed` outcome — the recovery verb is
`resume`. Resending would append your message a second time to a
conversation that already contains it; resuming continues from the last
committed boundary — committed tool effects are in the record, uncommitted
ones never happened. Only a fault that *threw* (pre-progress, session
unchanged) is safely retried by calling `send` again.

A fault can reach you on either channel, split by one fact — did the turn
make progress first?

```baml
// After progress: the fault is a Failed OUTCOME at a committed boundary.
match (session.send(msg)) {
    let failed: ai.Failed => {
        match (failed.cause) {
            let limited: ai.RateLimited => {
                let wait = baml.time.Duration.from_milliseconds(limited.retry_after_ms ?? 1000);
                baml.sys.sleep(wait) catch (e2) { _ => null };
                session.resume()      // continues from the committed state
            },
            _ => throw_or_report(failed.cause),
        }
    },
    // ...the other four outcomes
}

// Before progress (nothing committed): the fault THROWS, and the session
// is provably unchanged.
let outcome = session.send(msg) catch (e) {
    let limited: ai.RateLimited => {
        let wait = baml.time.Duration.from_milliseconds(limited.retry_after_ms ?? 1000);
        baml.sys.sleep(wait) catch (e2) { _ => null };
        session.send(msg)             // nothing was appended; send again
    },
    let failure: ai.Failure => throw failure,   // your judgment: Refused,
                                                // InvalidRequest, ParseFailed —
                                                // replay cannot help these
    let unknown: baml.errors.UnknownError => throw unknown,
}
```

Catch most-specific first: name the concrete types *you* consider worth
another attempt — the error never decides for you — then the `ai.Failure`
arm for everything else, and the wrapper as the escape hatch. The static
channel is only `ai.Failure | baml.errors.UnknownError`; the concrete arms
are runtime refinements, and the same catch works for every provider.

> **Note:** this split is a guarantee, not advice. The append is the first
> commit of a continuation; once it (or any model step) succeeds, every
> later fault arrives as `Failed` at a committed state, and every genuine
> throw means the session did not change. There is no partially-advanced,
> undefined middle.

## Tool failures never throw

A tool that fails — bad arguments, a thrown `ai.Failure` from the handler,
an unserializable result — becomes a `ToolError` submitted back into the
conversation, because the model is the party that recovers: it reads the
message, fixes its arguments, or tries another tool.

```baml
/// Tools raise failures by throwing; invoke_tool reifies the throw.
function read_file(path: string) -> string {
    // returning an explanatory string, or throwing an ai.Failure,
    // both reach the model as content it can react to
}
```

If a tool's failure should *stop the run* instead of informing the model,
that is an application decision: block it in a `before_tool_call` callback,
or check results in `after_tool_call` and throw from there.

## Treat `Unsupported` as a fact, not a failure

`baml.errors.Unsupported` always means a capability gap: this provider
cannot append to a conversation, serialize a session, or run an agent loop.
It is never transient. Don't retry it — switch providers or feature-gate:

```baml
let token = session.save() catch (e) {
    let unsupported: baml.errors.Unsupported => {
        log.warn("provider cannot persist sessions; continuing in-memory");
        null
    },
}
```

## Verify restores at the boundary

`AgentSession.restore` checks the token↔task pairing *before* any model
request, so a mispairing fails at the line that caused it — not as odd
model behavior three turns later:

```baml
let session = ai.run.AgentSession<Resolution>.restore(ResolveTicket@task(sample_ticket()), token) catch (e) {
    let mismatch: ai.run.SessionMismatch => {
        log.warn(`token saved under ${mismatch.expected ?? "?"}, got ${mismatch.found ?? "?"}`);
        fresh_session()
    },
    let unsupported: baml.errors.Unsupported => fresh_session(),
}
```

## Retry, fallback, and side effects

`ai.retry` and `ai.fallback` never learn concrete error types. They consult
one fact — `effects()` — plus a judgment: `ai.retry(provider, attempts,
retry_if = ..., backoff = ai.Backoff.default())` takes a predicate that
decides which failures are worth replaying. With no predicate, the default
judgment declines `Refused`, `InvalidRequest`, and `ParseFailed` and replays
the other effect-safe failures (including `NetworkFailure`) up to the
attempt cap, sleeping between attempts per the exponential `Backoff` — a
provider `RateLimited.retry_after_ms` hint overrides the computed delay.
Effect safety is enforced regardless — a failure reporting `Committed` or
`Unknown` effects is never replayed, whatever your predicate says. Three
rules they obey:

- **Retry wraps only the current provider `step`** — it never replays the
  agent loop or application tools. A replay-safe failed step must leave its
  conversation unchanged; that atomicity is an adapter obligation.
- **Exhaustion keeps the real error.** When retry gives up or every
  fallback member fails before progress, the last classified failure is
  rethrown intact — an outer catch still sees `ai.RateLimited`.
- **After any successful model turn, fallback keeps the selected provider**
  and rethrows later failures rather than restarting the conversation on a
  new member.

For failures *between* turns, session-level recovery is the counterpart:
back off and `resume`, as shown above.

## Define your own errors

Implement the one-method interface; no registration. The error rides the
channel because it implements `Failure`; retry and fallback classify it
through the interface; the application that knows the type catches it
concretely:

```baml
class VendorQuotaExceeded {
    quota_name: string,
    transient: bool,     // the vendor's own data — consumed by YOUR predicate
}

implements ai.Failure for VendorQuotaExceeded {
    function effects(self) -> ai.Effects throws never { ai.Effects.None }
}
```

```baml
//# Retryability is the app's judgment, reading its own error data
let judgment = (failure: ai.Failure) -> bool {
    match (failure) {
        let quota: VendorQuotaExceeded => quota.transient,
        _ => true,
    }
};

let resolution = ResolveTicket@task(ticket)
    .with_provider(ai.retry(limited_provider, 3, retry_if = judgment))
    .complete() catch (e) {
    let quota: VendorQuotaExceeded => {
        log.warn(`quota ${quota.quota_name} exhausted; escalating`);
        ResolveTicket@task(ticket).with_provider(careful_model()).complete()
    },
    let failure: ai.Failure => throw failure,
    let unknown: baml.errors.UnknownError => throw unknown,
}
```

Because *your* predicate returns `false` for this quota error, retry
refuses to replay it: the catch sees the concrete type after exactly one
attempt. The error never classified itself — it carried the data, and the
application decided.

## Unknown errors: wrap, annotate, recover

Anything foreign — an HTTP client's exception, a JSON parse error, a
subprocess exit — is normalized where it appears:

```baml
} catch (e) {
    let known: ai.Failure => throw known,
    _ => throw baml.errors.UnknownError.with_message<never>(e, "openai step"),
}
```

`with_message` never wraps twice; each layer adds one breadcrumb, so
context accumulates as the error bubbles:

```console
UnknownError { data: <socket reset>, message: ["transport", "retry(openai)"] }
```

At the handler — and only there — `UnknownError.from<T>` recovers a typed
error you expect:

```baml
match (baml.errors.UnknownError.from<VendorQuotaExceeded>(e)) {
    let quota: VendorQuotaExceeded => log.warn(`quota ${quota.quota_name} exhausted`),
    let unknown: baml.errors.UnknownError => throw unknown,
}
```

Never construct `baml.errors.UnknownError` directly; the normalize catch is
what keeps wrappers from nesting.

## Quick reference

| You caught | It means | Do |
| --- | --- | --- |
| `IncompleteRun` | run stopped; committed state inside | `AgentSession.of` + `resume`/`submit_tool_results`, or accept the stop |
| `Failed` outcome (not caught — matched) | fault after committed progress | inspect `failed.cause`; `resume` when your judgment says retry |
| `SessionBusy` | another continuation is in flight on this session | wait for it, or `fork()` earlier if you need parallel branches |
| `RateLimited` / `NetworkFailure` | provider hiccup | back off, `resume` — never resend |
| `Refused` / `InvalidRequest` / `ParseFailed` | replay cannot help | escalate, repair, log, or rethrow |
| `Unsupported` | capability missing on this provider | switch provider or feature-gate; never retry |
| `SessionMismatch` | token↔task pairing wrong | fix the re-supplied task, or start fresh |
| `UnknownError` | unclassified, wrapped at a boundary | log breadcrumbs; `UnknownError.from<T>` if you expect a type |

One sentence for the whole model: **errors report facts, you make the
judgments — match outcomes when stopping is your program's business, catch
`IncompleteRun` when it isn't, resume rather than resend, and treat
`Unsupported` as a fact about the provider rather than a failure of the
run.**

Runnable examples:

```console
baml run --from crates/baml_tests/baml_src_temp2 \
  ai_scenarios.agent_session

baml run --from crates/baml_tests/baml_src_temp2 \
  ai_scenarios.fallback_between_providers

baml run --from crates/baml_tests/baml_src_temp2 \
  ai_scenarios.switch_provider_after_failure
```
