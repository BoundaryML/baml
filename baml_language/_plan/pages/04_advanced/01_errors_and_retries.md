# Errors and retries

## The error model

Most errors in an agent are not thrown to your code. The loop absorbs
them and shows them to the model, because the model is the first retry
mechanism: a failed tool call or a malformed output is information it can
act on. Only errors the loop cannot absorb — exhausted budgets, dead
providers, cancellation — surface to the caller as ordinary BAML errors,
handled with `catch` / `catch_all`.

## The error catalog

Errors thrown to the caller:

| Error | Thrown when | Typical handling |
|---|---|---|
| `baml.errors.ParseError` | The model's final output failed schema parsing after repair and feedback retries. | Fallback value, or re-run with a different client. |
| `baml.session.StepBudgetExceeded` | The loop hit `max_steps` without producing the return type. | Raise the budget, tighten the prompt, or fall back. |
| `baml.session.CostBudgetExceeded` | `with_budget` middleware crossed its cost cap. | Raise the cap, or fall back. |
| `baml.errors.RateLimited` | Provider 429 after the client's retry policy is exhausted. | Backoff at the application level, or fallback client. |
| `baml.errors.AuthFailed` | Provider rejected credentials. Not retried. | Configuration fix; nothing programmatic. |
| `baml.errors.ProviderDown` | Network failures / 5xx after retries. | Fallback client. |
| `baml.session.Interrupted` | The session was interrupted while this call was waiting on it. | Usually intentional; report. |
| `baml.session.InstanceExists` | `$new = true`, or a job started with a taken ID. | Attach to the existing instance instead. |
| `baml.session.NotFound` | `resume` / attach with an unknown ID or corrupt snapshot. | Treat as fresh, or surface. |
| `baml.session.Busy` | An operation on a named instance while another writer holds its lease. | Wait and retry, or route through the running instance. |
| `baml.session.ToolNameConflict` | Two tools with the same name mounted on one toolbox. | Configuration fix at mount time; never reaches the model. |

Session errors chain their causes. `catch (e, ctx)` gives you the
`ErrorContext`: a `StepBudgetExceeded` whose last turn died on a rate
limit has that `RateLimited` reachable via `ctx.root_cause()`, so the
journal and the error tell one story.

Errors absorbed by the loop (recorded as events, shown to the model,
never thrown):

| Failure | Event | What the model sees |
|---|---|---|
| Bad tool arguments (wrong type, unknown parameter) | `ToolFailed` | The validation error; it retries with corrected arguments. |
| Tool threw | `ToolFailed` | The error message; it retries or takes another approach. |
| Unknown tool name | `ToolFailed` | The name was wrong plus the available tools. |
| Malformed output, first N attempts | (within the turn) | The parse error; it reformats. |
| Denied approval (`with_approval`) | `ToolFailed` | "Denied by operator"; it plans around the denial. |

Two error classes common in agent SDKs do not exist here, on purpose:
tool *output* validation and tool output serialization. BAML tools are
typed functions — a tool cannot return the wrong shape and every value
serializes. The exception is MCP tools, whose outputs are validated at
runtime against the server's schema; a mismatch becomes an ordinary
`ToolFailed`.

Settlement outcomes for jobs and named instances are values, not throws:
`poll()` returns `Failed { error, reason }` / `Aborted`. `reason`
distinguishes how it failed — `retry_exhausted` (attempt budget spent),
`timeout` (wall-clock budget spent), `needs_input` (the model stopped
conversationally with nobody to answer) — and `error` carries the same
catalog entries as data.

Over HTTP (`baml serve`), the catalog maps onto status codes:
`NotFound` → 404, `InstanceExists` → 409, `Busy` → 409, malformed
requests → 422, `RateLimited` passthrough → 429, settlement failures →
200 with the `Failed` value (the request succeeded; the work did not).

## Retries, layer by layer

Each layer retries the failures it can judge, and escalates the rest.

**1. Transport — the client.** Network errors, 429s, and 5xxs are retried
inside `invoke` with backoff. Configuration lives on the client, where it
always has in BAML:

```baml
client<llm> Primary {
    provider: anthropic,
    options: { model: "claude-sonnet-5" },
    retry_policy: { max_attempts: 3, backoff: "exponential", base_ms: 500 },
}
```

**2. Provider failure — fallback clients.** A fallback client tries
providers in order; the journal records which one actually answered
(every `AssistantMessage` carries its producer's ID, so mixed-provider
histories already render correctly):

```baml
client<llm> Robust {
    provider: fallback,
    options: { strategy: [Primary, Backup] },
}
```

**3. Output parsing — the turn.** A schema mismatch is repaired by the
parser when possible; otherwise the error is fed back and the model
reformats, up to a per-function attempt limit. These retries happen
inside one logical turn and are visible in the journal.

**4. Tools — policy middleware.** Transient tool failures are retried by
`with_retry`. The policy stays pure: it does not sleep, it emits a
command with a delay, and the runner owns the clock:

```baml
class WithRetry {
    inner: baml.session.Policy,
    max_attempts: int,
    tools: string[],           // which tools are safe to retry

    implements baml.session.Policy {
        function update(self, st: SessionState, j: Journal, e: Event) -> Command[] {
            match (e) {
                let f: ToolFailed if self.tools.includes(tool_of(j, f.call_id)) => {
                    //# attempts are counted from the journal, not from policy state
                    let n = j.entries.filter((en) -> {
                        match (en.event) {
                            let x: ToolFailed => x.call_id == f.call_id,
                            _ => false,
                        }
                    }).length();
                    if (n < self.max_attempts) {
                        [RetryTool { call_id: f.call_id, after_ms: 500 * n }]
                    } else {
                        self.inner.update(st, j, e)      // give up; let the model see it
                    }
                },
                _ => self.inner.update(st, j, e),
            }
        }
    }
}
```

Only list tools that are idempotent. Retrying a `charge_card` tool is how
customers get charged twice; for effectful tools, rely on steps and
idempotency keys instead (`../02_guides/12_durability.md`).

**5. Submissions — durability.** For jobs and named instances, the
runtime retries interrupted attempts under a per-function budget
(`max_attempts`, `timeout_ms`) until the submission settles. See the
durability notes.

## Letting the agent fail well

The most useful error handling is often in the type. If a task can be
legitimately impossible, say so in the return union instead of forcing
the model to fabricate or time out:

```baml
class CannotPlan {
    reason: string,
    missing_info: string[] @description("what the user would need to provide"),
}

function PlanTrip(trip_request: string) -> Itinerary | CannotPlan {
    // ...
}
```

The loop treats any member of the return union as a valid final result.
`CannotPlan` arrives as `Done`, typed, with a reason — instead of a
`StepBudgetExceeded` after twelve wasted turns.

## Handling failure at the call site

Ordinary BAML error handling, at the outermost sensible point:

```baml
let trip = PlanTrip(trip_request) catch_all (e) {
    let t: baml.session.StepBudgetExceeded => fallback_itinerary(trip_request),
    let p: baml.errors.ParseError => fallback_itinerary(trip_request),
    _ => throw e,
};
```

Catch around the agent call, not inside tools — tools should throw
freely, because their errors are information for the model.

## Everything lands in the journal

Every failure in this page is either an event (`ToolFailed`, settlement)
or occurs adjacent to recorded evidence (retry attempts, fallback client
IDs on `AssistantMessage`, `Usage` for every attempt). "Why did this run
fail" is answered by reading the journal, not by reproducing the failure.
