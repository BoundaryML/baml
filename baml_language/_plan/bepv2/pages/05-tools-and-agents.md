# 5. Tools and Agents

An agent, in practice, is a task plus a tool roster plus an execution
policy. This page builds that up: declaring tools, attaching them to a task,
running the loop, routing on its outcomes, and the boundary between tools
your app executes and tools the provider executes.

## Declaring a tool

A tool is a name, a description, and a parameter schema. The schema comes
from an ordinary BAML class — you never hand-write JSON Schema:

```baml
class WeatherArgs {
  city: string,
  days: int,
}

let weather_tool = baml.ai.Tool.from_type(
  "get_weather",
  "Get the weather forecast for a city.",
  reflect.type_of<WeatherArgs>(),
)
```

The stored type does double duty: outbound, it lowers to each provider's
wire schema dialect (JSON Schema for OpenAI/Anthropic, the OpenAPI subset
for Gemini — below the seam, not your problem); inbound, it validates the
model's arguments before your code sees them.

With first-class function types (BEP-062), the declaration collapses to the
handler itself — name from the function, description from its docstring,
schema from its signature:

```baml
let tools = [
  baml.ai.tool((a: WeatherArgs) -> string { forecast(a.city, a.days) }),
  baml.ai.tool((a: SearchArgs) -> string { search(a.query) }),
]
```

This is the intended end state: **the function is the tool.** Until BEP-062
lands, tools carry a schema and dispatch routes by name (below).

## The two rosters

Tools have two legitimate owners, and conflating them causes real bugs.

**Task-owned tools** are functions *your application* executes. They belong
on the task, in a `tools:` field:

```baml
function ResearchQuestion(q: string) -> Answer {
  client: ToolModel
  tools: [search_tool(), calculator_tool()]
  prompt: `Research ${q}. ${ctx.output_format}`
}
```

They travel with the task: `ResearchQuestion(q, client = Careful)` swaps the
model and **keeps the tools**, because the roster is task data, not
deployment data.

**Provider-owned tools** run on the vendor's servers — web search, code
execution, retrieval — with no client-side dispatch. They are provider
configuration, typed fields on the provider class:

```baml
let Grounded = baml.ai.Gemini { ...GeminiBase, grounding: true }
let answer = ResearchQuestion(q, client = Grounded)
```

They travel with the model choice — swap to a provider without server-side
search and the search rightly disappears. The provider merges both rosters
into the wire request; a name collision between them is a typed error, never
a silent shadow.

The test: *who executes it?* Your process → `tools:` on the task. Their
servers → a field on the provider.

## Running the loop: the plain call

A task with `tools:` runs the tool loop on a plain call:

```baml
let answer = ResearchQuestion("what changed in the Q3 numbers?")
```

Under the hood (page 2 shape): the driver sends the prompt with the tool
schemas; when the model requests calls, the dispatcher executes them and
returns results, preserving the provider's call IDs; the loop continues
until the model produces the final `Answer`.

The default dispatcher validates each call's arguments against the tool's
stored type. A mismatch does **not** abort the loop — it returns an error
result so the model can self-correct on the next turn.

Dependencies your tools need — database handles, the current user — ride
closure capture in the handler, never the model-visible arguments:

```baml
function research_tools_for(db: Database, user: User) -> baml.ai.Tool[] {
  [ baml.ai.tool((a: BalanceArgs) -> string {
      db.balance(user.id, a.include_pending)     // user.id injected, never in schema
  }) ]
}
```

## Routing on outcomes: `.agent`

A loop can end three ways: the model finished; the step budget ran out; the
model handed off. A plain call returns `Answer` — it can represent only the
first. When the other two are *expected control flow*, use `.agent` and get
the honest union:

```baml
match (ResearchQuestion.agent(q, budget = baml.ai.Budget { max_steps: 12 })) {
  let d: baml.ai.Done<Answer>    => d.value,
  let b: baml.ai.BudgetReached   => queue_for_review(b.transcript, b.steps_taken),
  let h: baml.ai.Handoff         => route_to(h.to, h.args),
}
```

Budget policy is a plain predicate — write the lambda inline; named
combinators are optional sugar:

```baml
ResearchQuestion.agent(q, budget = baml.ai.Budget {
  stop_when: (s: baml.ai.StepInfo) -> bool {
    s.steps_taken >= 12 || (s.cost_usd ?? 0.0) > 0.50
  },
})
```

### Return-type honesty: why both forms exist

The plain call cannot be widened to `Answer | BudgetReached`: a task's
declared return type is also its output schema, and the schema must not
advertise `BudgetReached` as something the *model* may produce. Nor should
budget/handoff be thrown — a planned stop is not a failure, generic catches
would discard the transcript (the paid-for partial work), and a retry
wrapper would re-drive the whole loop, re-executing every tool side effect.

So the rule is:

- **Budget is exceptional** (sized generously; hitting it is a bug) → plain
  call. Configure the budget policy as *graceful finish*: on exhaustion the
  loop injects one forced-synthesis turn ("answer now with what you have")
  and returns a real, degraded `Answer`, with
  `meta.attributes["budget_exhausted"]` set for callers who check.
- **Budget/handoff are control flow** → `.agent`, match the union.
- A task that participates in handoff **must** be called through `.agent`;
  a routing instruction is not an `Answer` and cannot be gracefully
  finished into one.

## The tools × modifier matrix (normative)

Declaring `tools:` changes what several modifiers mean, and two of them it
invalidates. This matrix is the contract; anything not listed here is
unspecified and must not be relied on.

| Modifier      | No task-owned tools            | Task-owned tools |
| ------------- | ------------------------------ | ---------------- |
| Plain call    | one `Generate` call → `T`      | agent loop, graceful finish → `T` (+ `budget_exhausted` meta) |
| `.agent`      | **compile error** (no roster and none supplied) | loop → `Done<T> \| BudgetReached \| Handoff` |
| `.with_meta`  | one response → `Response<T>`   | loop → `Response<T>` with **aggregate** meta (usage summed across turns, `attributes["steps"]`, per-turn detail in `raw`) |
| `.stream`     | `Stream<PartialT, T>`          | **compile error** (v1; see below) |
| `.background` | provider background job → `Job<T>` | **compile error** for task-owned tools (see below); valid when the loop is provider-executed |
| `.request`    | request, `tools: []`           | request with `tools:` roster attached |
| `.prompt`     | rendered prompt                | rendered prompt including tool schemas as the provider will see them |
| `.parse`      | parse final output             | parse final output (tool turns are not `.parse`'s concern) |

The two hard cells are errors on purpose, not gaps to paper over:

**Streaming a tool loop is not `Stream<Partial<T>, T>`.** A loop's timeline
is *model text → tool call requested → tool executing → tool returned →
model resumed → final value* — that is an event stream, not a sequence of
partials of `T`. Forcing it into the partial-stream type would either hide
the tool turns (lying about latency and cost) or corrupt the partial
semantics. So v1 rejects `.stream` on a tool task at compile time, with the
error naming the future path: a distinct `AgentEventStream<T>` whose items
are typed loop events with a final `T` — additive when designed, and not a
blocker for everything else.

**`.background` cannot run *your* tools.** Task-owned handlers execute in
your process; a provider background job runs on their servers after your
call returns — there is no process to dispatch the tool calls to. Making
this combination work requires a durable worker that BAML owns, which is a
workflow-engine concern, not a modifier. So v1 rejects `.background` on a
task with task-owned tools at compile time. Provider-*owned* tools are
fine — they execute server-side where the job lives:

```baml
function DeepResearch(topic: string) -> Report {
  client: baml.ai.Gemini { ...GeminiBase, grounding: true }   // provider-owned: OK
  prompt: `Research ${topic} thoroughly. ${ctx.output_format}`
}
let job = DeepResearch.background(topic, opts)                 // valid

function LocalOps(req: string) -> Report {
  client: ToolModel
  tools: [shell_tool(), db_tool()]                             // task-owned
  prompt: `...`
}
let job = LocalOps.background(req, opts)                       // COMPILE ERROR
```

## Custom loop policy: the capability, directly

Approval gates, per-call audit, tool filtering — anything that needs to see
each turn — drops one level to the `Tools` capability's step interface:

```baml
function run_with_approval(p: baml.ai.Tools, req: baml.ai.Request<Answer>, allow: string[]) -> Answer
    throws baml.errors.ToolError | baml.errors.UnknownError {
  let t = p.begin<Answer>(req)
  while (true) {
    match (p.step<Answer>(t)) {
      let calls: baml.ai.ToolCalls => {
        for (let c in calls.calls) {
          if (!allow.includes(c.name)) { throw ToolDenied { tool_name: c.name }; }
        }
        t = p.submit(t, dispatch(calls.calls));
      },
      let value: Answer => { return value; },
    }
  }
}
```

Note the signature demands `baml.ai.Tools`, not the existential — a function
that *needs* the capability says so in its type.

## Ad-hoc agent bundles: the `Agent` value

When the bundle is assembled at runtime — dynamic rosters, per-tenant
policies — build it as a value. `Agent` is a provider that runs the loop
itself, so any task accepts it as a client:

```baml
let support_agent = baml.ai.Agent {
  inner: ToolModel,
  tools: tools_for(tenant),
  stop_when: (s: baml.ai.StepInfo) -> bool { s.steps_taken >= 8 },
}

let answer = AnswerTicket(ticket, client = support_agent)
```

`Agent` composes with everything providers compose with — it can wrap a
spread-derived variant, sit inside a fallback, or be returned by a routing
function. Prefer the task `tools:` field when the roster is static; prefer
`Agent` when it is data. (Beware the one footgun: tools packed into a
*client* vanish when someone swaps the client. That is why the static home
is the task.)

## MCP and other tool sources

There is no special MCP mode, because MCP is a tool *source*, not a loop
shape. An MCP client yields a roster and a dispatcher; both plug into
everything above:

```baml
let mcp = baml.mcp.connect(server_url)

// task-level:
function OpsTask(req: string) -> Report {
  client: ToolModel
  tools: mcp.tools()
  prompt: `...`
}

// or ad-hoc:
let agent = baml.ai.Agent { inner: ToolModel, tools: mcp.tools(), dispatch: mcp.dispatch }
```

The same holds for tool registries, permission-filtered rosters, and
searchable catalogs: they produce `Tool[]` and `dispatch`; the loop is
indifferent to where they came from.

## Alternatives considered

The central choice on this page is where the roster lives. The proposal
picks the task field as primary; here is the full matrix, including the
design with **no task field at all**.

**No `tools:` field — rosters live only on `.agent` and `Agent` clients.**
In this design a task declaration never mentions tools; a plain call never
runs a loop; tooling is always explicit at the execution site:

```baml
// no tools: field anywhere
function ResearchQuestion(q: string) -> Answer {
  client: ToolModel
  prompt: `Research ${q}. ${ctx.output_format}`
}

// spelling 1: roster at the .agent call
let out = ResearchQuestion.agent(q, tools = research_tools(), budget = b)

// spelling 2: roster in the client value
let out = ResearchQuestion(q, client = baml.ai.Agent { inner: ToolModel, tools: research_tools() })
```

Genuine advantages: the function block stays minimal (no fourth field);
return-type honesty is automatic, because the loop only ever runs behind
`.agent`'s outcome union or an explicitly-constructed client — a plain call
is always a plain call; and there is no "does this task loop?" question to
answer from the declaration, because the answer is always no. This is the
right design *if* you believe rosters are usually assembled per call.

Why it is not primary: in practice rosters are stable per task — the
weather task always has the weather tools — and this design makes the
stable case pay the dynamic case's costs. The roster gets repeated at every
call site (spelling 1), or packed into the deployment slot where the
routine `client =` model swap silently destroys it (spelling 2). Two call
sites can drift to different rosters for the same task with no diagnostic.
And the task declaration stops being the complete description of the task —
tests, `baml describe`, and prompt-rendering tooling can no longer see what
tools the prompt will be accompanied by, which matters because tool
*presence changes model behavior* even before any call is made. Both
spellings survive in the proposal as the dynamic forms; the field is the
static default, same division as `client:` itself (declared default,
call-site override).

**Tools only at the call site** (`run_tools(req, tools, dispatch)` as the
sole spelling, no `.agent`, no field). The no-field design minus the
`Agent` value. Same drift and repetition costs, plus the loop becomes a
free-function-only surface — the discoverability problem the modifier set
exists to solve.

**Tools only on the client** (`client = Agent { ... }` as the sole
spelling). The most seductive alternative, because "the provider just runs
the loop" is architecturally clean — the call site stays a plain call and
the provider absorbs everything. Rejected as the *only* home for two
reasons: the client-swap footgun above (task data in the deployment slot),
and outcome routing — a plain call through an `Agent` client can only
return `Answer` or throw, so budget/handoff either become dishonest errors
or force graceful-finish semantics on everyone. Retained as the ad-hoc
form, where the bundle being a runtime value is exactly the point.

**A `.with_mcp` / `.run_with_mcp` modifier.** Rejected: it would encode a
tool *source* as an execution *mode*; the next source (a registry, a plugin
system) would demand another modifier. Sources produce rosters; rosters are
data.

**A static capability requirement on calls** ("if the client implements
`Tools`, the call must pass `tools =`"). Rejected: the client is swappable
at runtime, so the requirement is unknowable statically. The `tools:` field
gets the same safety declaratively — the roster is checked where it is
declared — and the `Agent` constructor gets it for the dynamic case (you
cannot build one without a roster).

**One roster for both owners** (model server-side tools as fake local
tools). Rejected: server tools have no client dispatch, different failure
modes, and different billing; pretending they are local calls forces a fake
dispatcher and breaks the "who executes it?" audit question.
