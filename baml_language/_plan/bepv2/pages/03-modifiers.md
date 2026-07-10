# 3. Modifiers

A modifier is a compiler-provided method on a task that changes its
execution lifecycle — and therefore its return type. The set is closed:
`.stream`, `.with_meta`, `.background`, `.agent`, `.request`, plus the
tooling pair `.prompt` and `.parse`. This page covers each with working
code, and when to reach for it.

Throughout, one running task:

```baml
class Plan {
  title: string,
  steps: string[],
}

function BuildPlan(goal: string) -> Plan {
  client: DefaultModel
  prompt: `Draft a plan for: ${goal}. ${ctx.output_format}`
}
```

Every modifier accepts the same user arguments as the task, plus its own
extras, plus the trailing `client =` override.

## The plain call — `BuildPlan(goal) -> Plan`

The default and the majority case. Use it whenever you want the answer and
the call can wait for it.

```baml
let plan = BuildPlan("migrate the billing service")

let plan = BuildPlan("migrate the billing service", client = CheapModel)

let plan = BuildPlan(goal) catch (e) {
  let u: baml.errors.Unsupported => fallback_plan(goal),
  _ => throw e,
}
```

Requires the client to implement `Generate`.

## `.stream` — `BuildPlan.stream(goal) -> Stream<PartialPlan, Plan>`

Use when values should arrive incrementally — UIs, long generations,
progress display. The partial type is compiler-derived from `Plan` (every
field optional, recursively), which is why streaming is a modifier and not a
library function: only the compiler can name that type for you.

```baml
let stream = BuildPlan.stream("migrate the billing service")

while (true) {
  match (stream.next()) {
    null => break,
    let partial: PartialPlan => ui.render(partial),
  }
}

let plan: Plan = stream.final()
```

Do **not** model a stream as repeated independent calls; that changes cost,
coherence, and semantics. Requires `Streaming`.

## `.with_meta` — `BuildPlan.with_meta(goal) -> baml.ai.Response<Plan>`

Use when you need the answer *and* out-of-band response data: token usage,
finish reason, reasoning traces, citations, logprobs.

```baml
let r = BuildPlan.with_meta(goal)

log.info(`plan used ${r.meta.usage?.input_tokens ?? 0} input tokens`)
let plan = r.value
let reasoning = r.meta.attributes.get("reasoning")
```

Metadata is produced once per call regardless; the plain call simply drops
it. `.with_meta` never repeats the model call. The `meta` shape is
normalized common data (`provider`, `model`, `finish_reason`, `usage`) plus
`attributes` / `raw` escape hatches for provider-specific dimensions.

Requires only `Generate` — same capability as the plain call, same wire
traffic, different caller contract.

## `.background` — `BuildPlan.background(goal, opts?) -> baml.ai.Job<Plan>`

Use when the work outlives the call: deep-research runs, hour-long
generations, anything you submit now and collect later. The return value is
honest — you do not have a `Plan`, you have a claim on one.

```baml
let job = BuildPlan.background(goal, baml.ai.BackgroundOptions {
  idempotency_key: "plan-" + ticket_id,
})
defer { job.cleanup() }

match (job.poll()) {
  let d: baml.ai.Done<Plan>  => save(d.value),
  let p: baml.ai.Pending     => persist(job.token()),   // resume in another process
  let f: baml.ai.Failed      => alert(f),
}
```

The idempotency key is how a retried submit avoids double-billing (page 8).
Persistence and resumption go through `job.token()` and the provider's
`resume_job` (page 6). Requires `Background`.

**Why a modifier and not a wrapper client?** Because the return type
changes. A "background provider" behind a plain call would have to either
block (not background) or lie about returning a `Plan`. Lifecycle changes
that change the caller's contract must be visible in the type; that is the
core rule of this design.

## `.agent` — `BuildPlan.agent(goal, budget?) -> Done<Plan> | BudgetReached | Handoff`

Use when the task has tools (page 5) and the caller routes on how the loop
ended. The plain call on a tool-equipped task runs the same loop but can
only return a `Plan` or throw; `.agent` returns the honest outcome union:

```baml
match (BuildPlan.agent(goal, budget = baml.ai.Budget { max_steps: 12 })) {
  let d: baml.ai.Done<Plan>      => d.value,
  let b: baml.ai.BudgetReached   => queue_for_review(b.transcript),
  let h: baml.ai.Handoff         => route(h),
}
```

Budget exhaustion and handoff are *expected control flow* here, so they are
sum arms, not throws. Requires `Tools` (or a client that runs the loop
itself). Page 5 covers the full story, including why the plain-call form is
only honest for tasks with graceful-finish budgets and no handoff.

## `.request` — `BuildPlan.request(goal) -> baml.ai.Request<Plan>`

The bridge out of the closed set. Use when the execution mode is not one of
the above: a custom capability, a session, a batch, a vendor-specific
operation, an inspection.

```baml
// custom execution mode (page 7):
let plan = run_moderated(BuildPlan.request(goal), "no-pii")

// provider-stored conversation (page 6):
let a = session.run(BuildPlan.request(goal))

// many independent requests, one batch:
let batch = baml.ai.submit_batch(BatchModel,
  goals.map((g) -> { BuildPlan.request(g, client = BatchModel) }),
  (req, i) -> { `goal-${i}` },
)

// vendor-specific experiment, no ceremony:
let out = Vendor.experimental_tree_search(BuildPlan.request(goal, client = Vendor), branches = 8)
```

`.request` performs the render but no I/O. If you find yourself rewriting a
task's prompt as a string to feed some API, the correct move is almost
always `.request` instead — it keeps the schema, roles, media, and identity
that the string would drop.

## Tooling modifiers — `.prompt` and `.parse`

Not execution modes; inspection and offline plumbing.

```baml
// see exactly what would be sent, without sending:
let ast = BuildPlan.prompt(goal)
log.info(baml.llm.prompt_to_text(ast))

// parse a stored/replayed model output without a network call:
let plan: Plan = BuildPlan.parse(saved_response_text)
```

`.parse` is what makes record/replay tests and log backfills cheap: the
task's schema-aligned parser is addressable without executing the task.

## Choosing, in one table

| You want | Use | Returns |
| --- | --- | --- |
| the answer | `BuildPlan(goal)` | `Plan` |
| incremental partials | `.stream` | `Stream<PartialPlan, Plan>` |
| answer + usage/citations/reasoning | `.with_meta` | `Response<Plan>` |
| submit now, collect later | `.background` | `Job<Plan>` |
| tool loop with routed outcomes | `.agent` | outcome union |
| any other consumer | `.request` | `Request<Plan>` |
| to look, not run | `.prompt` / `.parse` | `PromptAst` / `Plan` |

## Selector semantics (normative)

`Extract.stream` looks like member access, but it is not method dispatch on
a function value. Modifiers are **task companion selectors**: compile-time
name resolution against the task *declaration*.

**Resolution.** In the expression `Base.member(...)`, if `Base` resolves to
an LLM task declaration and `member` is in the closed modifier set, the
expression resolves to that task's generated companion. Otherwise normal
resolution (namespace access, field access, method call) applies unchanged.
Because the modifier set is closed and task declarations cannot contain
members, this rule can never shadow or be shadowed by user code.

**Generated names.** Each selector corresponds to a compiler-generated
sibling function with a stable internal name using the reserved sigil —
`Extract$stream`, `Extract$request`, `Extract$background`, ... These names
are what tooling, `baml describe`, and SDK codegen key on; the sigil
guarantees they cannot collide with user identifiers. Users write the dotted
form; the sigil form is an implementation detail (and internal plumbing like
`Extract$parse_stream` has no dotted spelling at all).

**Capture.** A selector *is* capturable as an ordinary function value; a
task value does *not* carry its modifiers:

```baml
let s = ExtractInvoice.stream      // valid: selects the companion function
let f = ExtractInvoice             // valid: the task as a plain function value
f(doc)                             // valid: calling the value
f.stream(doc)                      // ERROR: function values have no members
```

The asymmetry is deliberate. A function type erases which task (if any) a
value came from — `f` might be any `(pdf) -> Invoice` — so `f.stream` would
require task identity to survive type erasure, turning every function value
into a potential dispatch site. Selectors resolve on *names*, which the
compiler still has; values are just values. (Whether erased task values
should retain identity for future re-attachment is README open question 7;
nothing here precludes it.)

**Errors.** Selecting a modifier on a non-task (`helper.stream` where
`helper` is a plain function declaration) is a compile error naming the
selector and stating that `helper` is not an LLM task. Selecting an unknown
member on a task (`Extract.streem`) is a compile error listing the modifier
set — the misspelling case autocomplete exists for.

## SDK mapping

Host SDKs map modifiers to each language's established convention rather
than inventing one per mode:

```python
plan   = b.build_plan(goal)
stream = b.build_plan_stream(goal)
job    = b.build_plan__background(goal, idempotency_key=...)
req    = b.build_plan__request(goal)
```

```typescript
const plan = await b.BuildPlan(goal);
const stream = b.BuildPlan.stream(goal);
const req = b.BuildPlan.request(goal);
```

## Alternatives considered

**An open modifier set** (libraries add `.moderated` to every task).
Rejected: modifier names become global (two libraries cannot both define
`.moderated`), installing a dependency changes every task's members, and
SDK codegen must emit third-party members it cannot vouch for. The closed
set plus `.request` gives extensions a first-class path without any of
that; see README "Alternatives".

**`.background` as an option** (`BuildPlan(goal, background = true)`).
Rejected: the return type must change to `Job<Plan>`; an option that changes
the return type is a different function. Making it a distinct member keeps
both the type and the docs honest.

**Naming: `.meta` instead of `.with_meta`.** `.meta` reads as a property of
the task (its metadata) rather than an execution of it. `.with_meta` states
"run it, and keep the metadata."

**Naming: `.submit` instead of `.background`.** `.submit` describes the
first step, not the lifecycle; `.background` names what the caller signed up
for. (Batch submission is deliberately *not* a modifier — a batch is a
property of a collection of requests, not of one task; it takes
`Request<T>[]`.)

**Tooling modifiers as separate top-level functions**
(`baml.ai.render(BuildPlan, goal)`). Rejected: they need the task's private
plumbing (template, parser) anyway, so they are compiler-provided either
way; members keep them discoverable next to the thing they inspect.
