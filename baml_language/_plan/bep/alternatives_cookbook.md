# Alternatives Cookbook — one scenario, every spelling

> Working comparison doc (2026-07-09), not part of BEP-063. For each interaction
> shape it shows how the same user intent is spelled in:
>
> - **(A) Original design** — `llm-provider/ideas/` + `llm-provider-plan.md`:
>   capability interfaces used directly; companions as `Foo.stream`-style dotted
>   forms; `call_with` projections; data handles (`Job {id, owner}`).
> - **(B) Branch today** — what actually compiles on `aaron/custom-llm-providers`:
>   generated `$` companions (`Foo$stream`, `Foo$with`, `Foo$run_tools`, `Foo$live`),
>   the `//baml:llm_capability` / `//baml:llm_companion(suffix)` registry,
>   `ToolLoop`-style wrapper providers, direct capability methods with handles.
> - **(C) BEP-063** — `Foo$request` as the single generated seam; standard drivers
>   as free functions (`baml.ai.run_with_meta`, `submit_background`, ...);
>   provider-owned resource objects (`Job<T>`, `Session`, `LiveSession`).
> - **(D) Alternatives** — spellings none of the three chose (including the
>   `Response<T>` return type, method-style drivers on the request, `__` SDK-style
>   names, `run_with_mcp`, `infer`-style static capability requirements).

The recurring axis of disagreement is **where the execution mode lives**:

| Design | Mode lives in... | Consequence |
| --- | --- | --- |
| A | the capability method you call | explicit, but every call site is a `match` on the provider |
| B | a generated companion per (function × driver) | ergonomic, but N×M growth + a compiler registry |
| C | a free driver function over `Foo$request` | bounded codegen, but reading order inverts (`driver(Foo$request(...))`) |
| D1 (`Response<T>`) | the function's **return type** | mode becomes viral: every caller unwraps or matches |
| D2 (request methods) | methods on `LlmRequest<T>` | `Foo$request(x).run_with_meta()` — function first, mode last |
| D3 (wrapper provider) | the **client value** (`ToolLoop`) | plain `Foo(x, client = agent)`; mode invisible at the call |

---

## 1. Plain typed call

Identical in all three designs; this is the anchor everything else must not
disturb.

```baml
let invoice = ExtractInvoice(document)                 // A, B, C
let invoice = ExtractInvoice(document, client = Other) // dynamic override, A/B/C
```

- **A/B lowering:** orchestrator delegation / `drive_call(client, Foo$render_prompt(...))`.
- **C lowering:** `baml.ai.run(ExtractInvoice$request(document, client = DefaultModel))`.

No open questions here besides which lowering is easier to debug. C's is
strictly simpler to explain: one value (`LlmRequest<T>`) instead of a
`(client, PromptAst)` pair.

## 2. Value + metadata (usage, logprobs, citations, reasoning)

**(A) Original:** `call_with<T,V>(prompt, project)` — a projection callback runs
over the winner's `ResponseMeta`, returns `(T, V)`.

```baml
let (label, scores) = h.call_with<Label, Logprob[]>(p, (m) -> ... { m.logprobs() })
```

**(B) Branch:** the `$with` companion wraps the same projection:

```baml
let r = s08_Classify$with(
    review,
    (m: baml.ai.ResponseMeta) -> baml.ai.Logprob[] | baml.ai.Unavailable { m.logprobs() },
)
```

**(C) BEP-063:** metadata is always produced; `run` drops it, `run_with_meta`
keeps it. No projection callback (that keeps user code out of retry scopes).

```baml
let response = baml.ai.run_with_meta(SolveProblem$request(problem))
let answer = response.value
let reasoning = response.meta.attributes.get("reasoning")
```

**(D) Alternatives:**

- **D1 — declare it in the return type:** `function Solve(p: string) -> Response<Answer>`.
  Rejected (see opinions): the return type of an LLM function *is the schema*
  (`ctx.output_format`, SAP target, `$stream` partial derivation). Wrapping it in
  `Response<T>` either breaks that equation or forces the compiler to
  special-case-unwrap `Response<>`, and every caller pays `.value` (or a `match`
  for `Response<T> | T`) even when they never read metadata. Mode-of-one-call-site
  becomes the type of the function.
- **D2 — method on the request:** `SolveProblem$request(problem).run_with_meta()`.
  Same semantics as C, restores subject-first reading order. Costs: `LlmRequest<T>`
  grows a method per standard driver; custom capabilities remain free functions,
  so stdlib modes read postfix and user modes read prefix (an asymmetry, but
  arguably an honest one).
- **D3 — keep a `$with_meta` companion:** the N×M explosion argument only applies
  to *user* drivers; the stdlib set is fixed. BEP already concedes this for
  `$stream`. `Foo$with_meta(args)` would be one more bounded companion.

## 3. Streaming

All three designs converge — streaming keeps a generated companion because the
partial type is compiler-derived.

```baml
let stream = BuildPlan$stream(goal)          // B and C identical surface
// A spelled it Foo.stream(...) in the design prose; same idea.
```

- **(B)** `$stream` delegates to a registered driver.
- **(C)** `$stream` body ≡ `baml.ai.stream<Plan$stream, Plan>(BuildPlan$request(goal))`.
- **(D)** request-method form needs the partial type spelled by hand
  (`req.stream<Plan$stream>()`), which is exactly why the companion survives.

## 4. Tool calling / agentic loop

**(A) Original:** capability methods, hand-rolled or default loop:

```baml
match (p) {
  let tp: Tools => tp.run_tools<string>("go", tools, dispatch),   // default loop
  _ => throw Unsupported,
}
// or begin/step/submit for custom policy (approval gates etc.)
```

**(B) Branch:** three coexisting spellings —

```baml
// (i) the wrapper-provider: the LOOP IS THE CLIENT; the call site is a plain call
let agent = baml.ai.ToolLoop {
    inner: ScriptedTools { ... },
    tools: s09_weather_tools(),
    dispatch: echo_dispatch,
    stop_when: null,
};
let r = s09_Weather("Tokyo", client = agent)

// (ii) the generated companion
let outcome = s09_Weather$run_tools("Tokyo", tools, dispatch, stop_when = baml.ai.step_count_is(2))

// (iii) capability methods directly (begin/step/submit) for custom policy
```

**(C) BEP-063:** one free driver with an honest outcome union:

```baml
let outcome = baml.ai.run_tools(
  ResearchQuestion$request(question),
  [SearchTool, CalculatorTool],
  dispatch,
  baml.ai.ToolBudget { max_steps: 12 },
)
match (outcome) {
  ToolSucceeded<Answer> { value: let v } => v,
  ToolBudgetReached { transcript: let t } => queue_for_review(t),
  ToolHandoff { request: let r } => route(r),
}
```

**(D) Alternatives:**

- **D3 — keep `ToolLoop` as the *primary* spelling** (it already works on the
  branch and survives into C unchanged, since `ToolLoop` can implement
  `Generate`): tools/dispatch/budget are configuration of a provider *value*,
  so the call site stays `Foo(args, client = agent)`. `run_tools` remains the
  lower-level form for callers who want the outcome union in hand.
- **Naming: `Agent` / `run_agent`, not `ToolLoop` / `run_tools`.** Layer the
  vocabulary: the *capability interface* stays `Tools` (it names the wire
  protocol — tool-call turns — and capabilities model protocols), but the
  *app-facing value* should be `baml.ai.Agent { inner, tools, dispatch,
  stop_when }` (the ecosystem word for exactly this bundle: Pydantic AI,
  OpenAI Agents SDK, "models using tools in a loop"), and the driver, if it
  keeps a prefix form, reads better as `run_agent` — it names the behavior
  (loop to completion) where `run_tools` names the mechanism and misreads as
  "execute the tool calls". Bonus symmetry: with an `Agent` value,
  `agent.run(Foo$request(x))` lines up with `session.run(...)` and
  `cache.run(...)` into one uniform pattern — **`<context>.run(request)`** for
  every execution context, with free `baml.ai.run` as the no-context case.
  The outcome-union form stays available (`agent.run_to_outcome(request)` or
  the free driver) for callers handling budget/handoff explicitly.
- **D5 — a `tools:` field on the LLM function (proposed 2026-07-09).** Three
  candidate homes for the tool roster: the call site (`run_tools(req, tools,
  ...)` — every call carries task data), the client value (`client =
  Agent { inner, tools, ... }` — works today, but `client =` override is the
  most common operation in the language and silently *drops the tools* when
  someone swaps models, because task-owned data was packed into the
  deployment-owned slot), or the declaration:

  ```baml
  function ResearchQuestion(q: string) -> Answer {
    client: ToolModel
    tools: [SearchTool, CalculatorTool]        // task-owned; survives client swaps
    prompt: `Research ${q}. ${ctx.output_format}`
  }
  ResearchQuestion(q, client = Claude)          // swaps model, keeps tools
  ```

  The compiler lowers the main body through the agent loop when `tools:` is
  present; `$request` carries the roster so drivers/custom capabilities see
  it. With BEP-062 the list is bare function references and dispatch defaults
  away. Caveat: a plain `-> Answer` call can only return or throw, so
  budget/handoff become errors in this form — see §4b for the full analysis;
  keep the explicit driver for callers that route on the outcome union.

  **Two rosters, not one.** Providers also ship their *own* tools — Gemini
  web search / code execution, OpenAI hosted tools — executed server-side
  with no client dispatch (scenario 7: that stays `Generate` + provider
  config, e.g. `Gemini { grounding: true }`). So the split is:
  *provider-owned tools = client config* (they travel with the model choice —
  swap to Claude and Gemini's search rightly disappears);
  *app-dispatched tools = `tools:` on the task* (they travel with the task).
  The provider merges both lists into the wire request. Residue to spec:
  name collisions between the two rosters (provider should error, not
  shadow), and mixed loops where one turn contains a server-executed call
  and an app-dispatched call — the `Tools` capability sees only the latter.
- **`run_with_mcp`:** not needed as a driver. MCP is a *tool source and
  dispatcher*, not a loop shape: `run_tools(req, mcp.tools(), mcp.dispatch)`
  or `ToolLoop { tools: mcp.tools(), dispatch: mcp.dispatch, ... }`. The loop
  protocol is identical; only where tools come from differs.
- **Static requirement (`infer`)**: "if the client is a Tools provider, the
  call must supply `tools =`" is flow-dependent typing — the client can be
  swapped at runtime, so the compiler cannot know. The two honest ways to get
  static checking exist already: (i) a narrower parameter type
  (`fn drive(p: baml.ai.Tools, ...)`), (ii) a wrapper value whose
  *constructor* demands the tools (`ToolLoop { tools: ..., dispatch: ... }`)
  — the requirement is checked where the value is built, not where the
  function is called.
- **Tool carries its handler** (needs BEP-062 function types): today `Tool`
  is `{name, description, parameters}` and dispatch switches on `call.name`
  by hand — which is why `echo_dispatch` in the fixtures never *runs*
  anything. With first-class function types stored in a heterogeneous list:

  ```baml
  let tools = [
    baml.ai.tool("get_weather", "…", (a: WeatherArgs) -> string { lookup(a.city) }),
    baml.ai.tool("forecast",    "…", (a: ForecastArgs) -> string { fc(a) }),
  ]
  let outcome = baml.ai.run_tools(req, tools)   // default dispatcher invokes handlers
  ```

  This is the single biggest ergonomic win available in the tools area and is
  blocked only on BEP-062 (erasing `(A) -> R` behind an existential so the
  list is homogeneous, then SAP-coercing args to each handler's declared type,
  which is already the D6 story).
- **`stop_when` combinators vs lambdas:** `baml.ai.step_count_is(2)` is a
  named predicate factory. Since `stop_when` is just `(StepInfo) -> bool`,
  examples should teach the lambda first; factories are optional sugar:

  ```baml
  stop_when = (s: baml.ai.StepInfo) -> bool { s.steps_taken >= 2 }
  ```

## 4b. Return-type honesty — three outcomes, two channels

Why "plain call runs the loop" (ToolLoop client, declarative `tools:`) can
never fully replace the explicit driver. An agent loop has three terminal
outcomes:

```text
1. final answer  — model stopped calling tools, produced a parseable Answer
2. budget stop   — stop_when fired; N steps of transcript + side effects exist
3. handoff       — "route this to a different agent / a human"
```

A plain call has two channels — return (typed `Answer`) and throw — and the
return channel is doubly constrained because **an LLM function's declared
return type is also the output schema**. Widening to
`-> Answer | ToolBudgetReached` would render `ToolBudgetReached` into
`ctx.output_format` and let SAP accept one off the wire; there is no syntax
separating "types the runtime may produce" from "types the model may
produce". (Same coupling that rules out `Response<T>`, §2 D1.) A free driver
has no such constraint — its return type is just a type — which is the whole
reason `ToolSucceeded<T> | ToolBudgetReached | ToolHandoff` lives there.

Forcing outcomes 2/3 through `throws` replays the original plan's D5 sin:

```baml
class BudgetReached {
  transcript: baml.ai.ChatMessage[],
  steps_taken: int,
  implements baml.errors.ToolError {
    function is_network_error(self) -> bool { false }   // vacuous — nothing
    function is_rate_limit(self)    -> bool { false }   // here describes a
    function is_parse_error(self)   -> bool { false }   // PLANNED stop
  }
}

// generic catch discards the N steps of paid-for work:
let a = ResearchQuestion(q) catch (e) { _ => default_answer() };

// retry combinator re-drives the WHOLE loop — re-executes every tool side
// effect, re-bills every turn, hits the same budget, throws again, 3×:
let a = ResearchQuestion(q, client = baml.ai.retry(agent, policy))

// wanting the partial work means using the error channel as a data channel:
let a = ResearchQuestion(q) catch (e) {
  let b: BudgetReached => resume_later(b.transcript, b.steps_taken),
  _ => throw e,
};
```

The D8 classifier axis (`is_policy_refusal: true`, `is_retryable: false`)
upgrades the retry hazard from silent to typed-and-refused, but the semantics
still lie: nothing failed. (BAML `throws`/`catch` is structurally an
out-of-band typed return — a Rust-style side channel — which is exactly why
it is *tempting* to route outcomes through it, and why the D5 rule matters:
an outcome that has a value is a sum arm; only a genuine failure is a throw.)

**The escape hatch that keeps the plain call honest: graceful finish.** Define
the budget policy so the loop always produces an `Answer` — on budget hit,
inject a forced-synthesis turn ("answer now with what you have") and record
budget-ness as metadata (`meta.attributes["budget_exhausted"]`). Degraded but
truthful. Handoff, however, cannot be gracefully finished — a routing
instruction is not an `Answer`.

| Outcome | Plain call + throw | Plain call + graceful finish | Driver union |
| --- | --- | --- | --- |
| Final answer | fine | fine | fine |
| Budget stop | typed error; retry hazard; data rides error object | forced `Answer` + meta flag; degradation invisible unless checked | first-class arm |
| Handoff | typed error (semantically absurd) | **impossible** | first-class arm |

Rule: the declarative form is honest for tasks with graceful-finish budgets
that never hand off. A task that hands off is structurally multi-outcome and
must be called through the driver — no single-typed plain call can represent
it without lying on some channel.

## 5. User-defined capability (moderation as the example)

**(A) Original:** interface + runtime match, called directly. No codegen story.

**(B) Branch:** registry markers + a generated companion per LLM function:

```baml
//baml:llm_capability
interface Moderated requires baml.ai.Provider { ... }

//baml:llm_companion(moderated)
function drive_moderated<T>(client, prompt, policy: string) -> T { ... }

ComposeNote$moderated("turtles", "no-pii", client = GuardedEcho { ... })
```

**(C) BEP-063:** the driver is an ordinary generic function over the request;
no markers, no registry, no synthesized symbol:

```baml
run_moderated(ComposeNote$request("turtles", client = GuardedEcho { ... }), "no-pii")
```

**(D) Alternatives:**

- **Wrapper provider instead of a capability** — when the shape is still
  `LlmRequest<T> -> LlmResponse<T>`, don't mint a capability at all:
  `ComposeNote(x, client = GuardedProvider { inner: OpenAiModel, policy })`.
  (C makes this the recommended default; the custom capability is only for
  genuinely new lifecycles.)
- **`$using(mode)`** — rejected in the BEP: mode return types vary
  (`T` / `Stream` / `Job<T>` / unions), so a generic `$using` needs
  higher-kinded output types.

## 6. Background jobs

**(A/B) Original + branch:** capability verbs with a data handle; caller
re-associates handle ↔ provider by hand.

```baml
let job: baml.ai.Job<string> = b.submit<string>("big architectural review", "req-42")
let first = b.poll<string>(job)          // pending -> done; owner guard is runtime-checked
```

**(C) BEP-063:** driver + resource object; polling/cancel/cleanup live on the
job; persistence is an explicit token.

```baml
let job = baml.ai.submit_background(
  DeepResearch$request(topic, client = BackgroundModel),
  baml.ai.BackgroundOptions { idempotency_key: research_id },
)
let result = job.poll()
let saved = job.token()                       // serializable, non-secret
let resumed = LongRunningModel.resume_job<Review>(saved)
```

**(D) Alternatives:** none seriously competitive. The A/B handle forces the app
to carry {id, owner, parser, lifecycle} as folklore; the resource carries them
as fields. The only cost of C is that a resource is not directly serializable —
hence the token()/resume split, which is honest about the two different jobs.

## 7. Sessions, server-stored chains, fork

**(A/B):** state split across provider methods and handles:

```baml
c.chat<string>("what did we discuss?", baml.ai.Session { _id: "sess-1" })
c.extend<string>("and its population?", handle)               // Chain
let f = b.fork(baml.ai.Session { _id: "root" })               // Branching
```

and the manual continuation the branch's scenario 19 falls back to —
`s19_run_branch(p, prefix, id, continuation)` matching `HttpProvider` and
calling `call_messages<string>(messages)` — i.e. the app re-implements
"session" out of loose parts.

**(C) BEP-063:** the provider returns a `Session` resource; requests run
*through* it; forking returns another resource:

```baml
let session = baml.ai.open_session(SessionModel, baml.ai.SessionOptions {})
defer { session.cleanup() }

let greeting = session.run(Greet$request(name, client = SessionModel))
let alt = session.fork()
let a = session.run(Choose$request("conservative"))
let b = alt.run(Choose$request("experimental"))
```

**(D) Alternatives / open ergonomic issue:** `client = SessionModel` inside a
request that is then run *by* the session is redundant and a footgun (the
session validates ownership at runtime). Since `LlmRequest.for_provider`
exists, `session.run` can rebind the request to the session's owner
automatically, letting users write `session.run(Greet$request(name))`. That
should probably be the specified behavior, with a typed error only when the
request was *explicitly* bound to a different provider.

## 8. Realtime

**(A/B):** `Realtime.run(prompt: string, io: Channel)` plus provider-level
`LiveControl.cancel/truncate(channel)` — control targets a channel and hopes it
maps to the right provider session.

**(C):** `open_live(request, channel) -> LiveSession`; `cancel_response()` /
`truncate_assistant_audio(ms)` are methods on the session that owns the socket.

No credible (D); the resource form is strictly better here.

## 9. Retry / fallback / routing

**(A/B):** generic combinator classes (`Retry`, `Fallback`) that implement the
full `HttpProvider` codec, forward or throw per method, and consult a
provider-wide `is_effectful()`.

**(C):** capability-specific wrappers (`RetryGenerate`) + per-operation
`ReplayPolicy` + per-error commit state. Business routing stays ordinary code:

```baml
let provider = baml.ai.retry(primary, baml.ai.RetryPolicy { max_attempts: 3, ... })
let result = ExtractInvoice(document, client = provider)
```

**(D):** the original plan's D2/D8 "one classifier axis" (`is_retryable` on
errors + `is_effectful` on providers) is the same idea as C's
`ReplayPolicy`/`CommitState` with different placement; C's placement (on the
operation, not the provider) survives providers that mix safe reads with
never-replay live sessions.

**(D2 — capability-scoped builder methods: seductive, but narrowing.)**
One could hang `with_retry` as a default method on each *capability* instead
of the marker (`Generate.with_retry(policy) -> Generate`, `Background`'s
variant demanding an idempotency key, `Realtime` having none) — refusal
becomes a compile error and the per-capability signatures encode the replay
rules statically. **The catch is capability transparency**: a concrete
`OpenAi{...}` is `Generate` *and* `Streaming` *and* `Tools`; wrapping it in a
`Generate`-typed retry silently discards the siblings — `Foo$stream(q,
client = wrapped)` fails even though the inner streams fine. A wrapper can
only preserve the inner's surface by *claiming* it and checking at runtime —
which is why **both** scenario 29s (original `implement.baml:366` and branch
stdlib `combinators.baml`) implement `Retry { inner: Provider }` with a
`match` inside every forwarded method, and why that is not naive: it is the
only capability-preserving combinator expressible without intersection/Self
types (`with_retry(self) -> typeof(self)` — the D3 type-system ask).

Where the refusal happens is the real axis:

| Design | Refusal point | Preserves inner surface? |
| --- | --- | --- |
| `Retry { inner: Provider }` + `is_effectful` (A/B) | call time | yes (claims all, checks at runtime) |
| Same wrapper, semantic capabilities + `ReplayPolicy` (C) | call time, per honest operation | yes; and Generate = 1 method, so no fake codec stubs |
| `Generate.with_retry -> Generate` (D2) | compile time | **no — narrows** |

C is A/B refined, not replaced: the existential wrapper survives; semantic
capabilities remove the fake `build_request`/`parse` stages, and per-op
`ReplayPolicy` replaces the provider-wide `is_effectful` bool. D2 is right
only where narrowing is the intent. General shape of the whole surface
remains: **task methods when holding a task, capability methods when holding
a concrete provider, `baml.ai.*` free functions as the negotiation layer for
existential `Provider` values** (~14 functions, mostly invisible behind the
first two surfaces).

## 10. Manual / raw request (no LLM function)

**(A/B):** `HttpProvider.call_messages<T>(messages)` / hand-built
`ChatMessage[]` (the scenario-19 branch helper).

**(C):** `baml.ai.request<T>(provider, prompt`...`)` — the manual twin of
`$request` — then any driver. Lazy `prompt` templates make this coherent
(`ctx.output_format` renders when `T` and provider are known).

**(D):** none needed; C subsumes A/B (`request.messages()` recovers the
message view for provider authors).

---

## Cross-cutting: the seam currency — and the honest line-count ledger

The single deepest change from A/B to C is not companions or resources; it is
the **currency at the capability seam**. A and B let capabilities accept
whatever was convenient — `call<T>(PromptAst)`, `call_messages<T>(ChatMessage[])`,
`submit<T>(prompt_text, key)`, `run_tools<T>("go", ...)` — three or four
currencies, bare strings included. C collapses every seam to one type,
`LlmRequest<T>`, produced by `Foo$request(...)` or manually by
`baml.ai.request<T>(provider, template)`.

Important correction to a common reading: **C does not force LLM functions**
(the manual constructor exists, and Rule 6 blesses direct provider methods for
non-LLM operations). It forces the *request value*. That costs lines in some
places and refunds them in others. The ledger, pattern by pattern:

### Where the BEP costs MORE lines

**(1) The ad-hoc string task — worst case, +4 to +8 lines.**
Branch today (scenario 27), one line, given a narrowed `b: Background`:

```baml
let job = b.submit<Review>("Review the last 90 days of churn data", "req-42")   // 1 line
```

BEP-063 — the task must exist as a function (or a manual template) first:

```baml
function ReviewChurn() -> Review {                                   // +4 lines, once per task
  client: BackgroundModel
  prompt: `Review the last 90 days of churn data. ${ctx.output_format}`
}

let job = baml.ai.submit_background(                                 // 4 lines vs 1, per site
  ReviewChurn$request(client = BackgroundModel),
  baml.ai.BackgroundOptions { idempotency_key: "req-42" },
)
```

~8-9 lines against 1. The asterisk that makes this comparison honest: the
branch one-liner is short partly because it is **wrong** — `submit<Review>`
takes a flattened string, so `ctx.output_format` is never rendered and parsing
`Review` from the reply relies on the model guessing the schema. About half of
the BEP's extra lines are buying the schema render, roles, and trace identity,
not ceremony. The other half (`BackgroundOptions {}` instead of a positional
key) is genuinely more verbose.

**(2) One extra nesting level at every advanced call site — ≈0 lines, +1 call.**

```baml
// branch:  s09_Weather$run_tools("Tokyo", tools, dispatch, stop_when = ...)
// BEP:     baml.ai.run_tools(s09_Weather$request("Tokyo"), tools, dispatch, budget)
```

Same line count, ~15 more characters, subject buried one level deeper (the
reading-order complaint; fixable with driver methods on the request, §2 D2).

**(3) The already-narrow quick call — ≈0 lines, longer expression.**
If you already hold `h: HttpProvider`, branch is `h.call_messages<string>(messages)`.
The BEP equivalent still constructs a request first:
`h.generate<string>(baml.ai.request<string>(h, prompt_from_messages(messages))).value`
— one line, but the provider is mentioned twice and there are two calls where
there was one.

### Where the BEP costs FEWER lines

**(4) Every dynamically-routed call site — −3 to −4 lines each.**
On the branch, *every* advanced call in app code wears the negotiation match
(scenarios 09, 17, 20, 27 all do this at every site):

```baml
let reply = match (p) {                                              // 4-5 lines of boilerplate
    let c: baml.ai.Chain => c.extend<string>("and its population?", handle),
    _ => throw baml.errors.Unsupported { message: "no chain capability" },
};
```

In the BEP the match lives once inside the stdlib driver; the call site is:

```baml
let reply = session.run(AskFollowup$request(question))               // 1 line
```

The scenario files are the evidence: strip the offline-fixture noise and most
"usage" tests are 60-70% match-and-throw scaffolding.

**(5) Custom-capability authors — −2 marker lines, minus a convention.**
Branch: `//baml:llm_capability` + `//baml:llm_companion(suffix)` + a driver
whose signature must follow the registry's `(client, prompt, extras...)`
convention. BEP: interface + ordinary driver over `LlmRequest<T>`. Two fewer
magic lines, and no signature convention to learn or for the compiler to
validate (the whole `capability_registry.rs` + PPIR synthesis path deletes).

**(6) Provider authors — same body, −3 method signatures.**
Branch `HttpProvider` demands `build_request` / `send` / `parse` /
`parse_meta` (+ `call_messages_with`); combinators then implement or
throw-stub each. BEP `Generate` is one `generate<T>` method whose body
contains the same stages as private code. Wrappers shrink the most: a
`Retry`/`Fallback` no longer implements four codec stages it must fake.

**(7) Metadata — −1 lambda and its type annotations.**

```baml
// branch: s08_Classify$with(review, (m: baml.ai.ResponseMeta) -> Logprob[] | Unavailable { m.logprobs() })
// BEP:    baml.ai.run_with_meta(s08_Classify$request(review)).meta
```

### Net ledger

| Pattern | Branch (B) | BEP (C) | Δ per site |
| --- | --- | --- | --- |
| Ad-hoc string task → submit/session | 1 line (schema-less) | 4-6 + 3-5 once | **+4..+8, C's worst case** |
| Advanced call, dynamic provider | 4-5 (match wrapper) | 1-2 | **−3, C's best case** |
| Advanced call, narrow provider in hand | 1 | 1 (longer, nested) | ≈ |
| Standard driver (tools/meta) | 1 companion call | 1 driver call, +1 nesting | ≈ |
| Custom capability, author side | interface + 2 markers + driver | interface + driver | −2 + no convention |
| Custom capability, call site | `Foo$moderated(x, p)` | `run_moderated(Foo$request(x), p)` | ≈, +nesting |
| Provider implementation | 4-5 required methods | 1 method | ≈ body, −3 signatures |

Summary judgment: C is a wash or a win everywhere except the **ad-hoc,
schema-less, string-prompt one-off**, which it makes 4-8 lines more expensive
on purpose — that pattern was cheap on the branch precisely because it dropped
the output schema, identity, and options on the floor. Whether that
enforcement is paternalism or hygiene is the real philosophical question; the
line counts themselves are not the argument against C that they first appear
to be.

## Cross-cutting: companion naming and the SDK

In-language, generated companions are spelled with `$` (`Foo$stream`,
`Foo$request`, `Foo$render_prompt`, `Foo$parse`) — `$` is the reserved
"compiler-generated sibling" marker and is unambiguous with user identifiers.

How that maps to host SDKs today (verified 2026-07-09):

- **Classic engine generators** (`engine/generators/languages/*`): mode-first
  namespace objects, function name last, no suffixes anywhere —
  `b.MyFunc(...)`, `b.stream.MyFunc(...)`, `b.request.MyFunc(...)`,
  `b.parse.MyFunc(...)` (TS/Python/Ruby; Go uses `Stream.MyFunc(ctx, ...)`).
- **New-compiler SDKs** (`baml_language/sdks/*`): suffix-preserving.
  - TypeScript keeps `$` verbatim: `foo$stream`, `foo$build_request`
    (plus `_async` siblings) — `sdkgen_typescript_node/src/emit/mod.rs:288`.
  - Python maps `$stream` → `foo_stream` (single underscore, blessed) and
    **every other suffix** `$x` → `foo__x` (double underscore):
    `extract_resume__build_request`, `extract__parse` —
    `sdkgen_python_pydantic2/src/emit/mod.rs:315`.

So `MyFunction__run_with_meta` would conform to the new Python convention,
`MyFunction$run_with_meta` to the new TS one, and `b.stream.MyFunction` to the
classic one. The BEP's `b.requests.ReviewRepository(...)` sketch follows the
*classic* namespace style, not the new suffix style — if the new-compiler SDKs
are the future, the BEP's host-SDK section should be rewritten to
`ReviewRepository$request` (TS) / `review_repository__request` (Python) for
consistency, or the new SDKs should adopt namespaces. Either is fine; having
both conventions live is the only wrong outcome.

## Scorecard (opinionated)

| Scenario | Easiest to understand | Notes |
| --- | --- | --- |
| Metadata | C, ideally as D2 (`req.run_with_meta()`) | A/B's projection callback is the hardest to teach and complicates retry scopes |
| Tools (app author) | D3 (`ToolLoop` client) | call site stays a plain function call; keep C's `run_tools` for outcome-union control |
| Tools (handlers) | D (BEP-062 `Tool.from_fn`) | dispatch-by-name is boilerplate that only exists because function types can't ride in `Tool` |
| Custom capability | C | registry markers (B) are the least explainable part of the branch |
| Background/sessions/realtime | C resources | A/B handles push lifecycle invariants into folklore |
| Streaming | tie | all designs kept `$stream` |
| Plain call | tie | must stay boring, and does |
