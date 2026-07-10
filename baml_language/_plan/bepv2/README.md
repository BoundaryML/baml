> **Status:** DRAFT — written for design review; proposed names do not yet compile on this branch.

# BEP-064: Tasks, Modifiers, and Providers

## Abstract

This BEP defines how BAML programs describe AI work and execute it in
different ways. It rests on one claim: **the unit of AI programming is the
task, not the model.** A task is an LLM function — a typed signature whose
implementation is a prompt. Everything else in this proposal exists to let
one task declaration be executed in every way a real application needs:
immediately, streamed, with metadata, in the background, as an agent, inside
a session — without rewriting the task and without the compiler generating an
unbounded API.

The proposal has five parts:

1. **Tasks.** An LLM function declares a typed task; its return type is the
   output schema; its client is a swappable default.
2. **Modifiers.** A small, fixed set of dot-methods on the task changes *how*
   it executes: `Extract.stream(doc)`, `Extract.background(doc)`,
   `Extract.with_meta(doc)`, `Extract.agent(doc)`, `Extract.request(doc)`.
3. **Requests.** `Extract.request(doc)` reifies one invocation — rendered
   prompt, output type, provider, options — as a value, `baml.ai.Request<T>`.
   Every modifier, every custom execution mode, and every provider consumes
   this one currency.
4. **Providers and capabilities.** A provider is an ordinary BAML class that
   implements only the capability interfaces it supports. Providers are
   values; deriving a variant is a one-line struct update:
   `OpenAi { ...Fast, model: "gpt-5-mini" }`.
5. **Resources.** Stateful operations return objects that own provider state
   and lifecycle: `Job<T>`, `Session`, `Live`.

Those five parts are the provider model. Durable workflows are an optional
orchestration layer above it, specified on page 10: workflows use tasks,
providers, capabilities, and resource tokens, while a separate executor owns
checkpoints, timers, signals, scheduling, and replay.

## Reading guide

Pages 1–9 build the provider model in order. Page 10 is the follow-on
orchestration design; an engineer new to both should still read front to
back.

- [1. Tasks and philosophy](./pages/01-tasks-and-philosophy.md) — why
  task-first; the comparison with agent-object libraries.
- [2. What a task desugars to](./pages/02-desugaring.md) — the complete
  lowering, from `Extract(doc)` down to the wire.
- [3. Modifiers](./pages/03-modifiers.md) — each modifier, when to use it,
  what it returns.
- [4. Clients and providers](./pages/04-clients-and-providers.md) — declaring,
  deriving, and swapping providers; dynamic clients.
- [5. Tools and agents](./pages/05-tools-and-agents.md) — the two tool
  rosters; the `.agent` modifier; return-type honesty.
- [6. Resources](./pages/06-resources.md) — background jobs, sessions,
  realtime, caches; tokens and resumption.
- [7. Custom capabilities](./pages/07-custom-capabilities.md) — extending the
  system in user code, no compiler hooks.
- [8. Reliability and errors](./pages/08-reliability-and-errors.md) — retry,
  fallback, the failure model, replay safety.
- [9. Normative signatures](./pages/09-normative-signatures.md) — the exact
  provider-layer contracts for every capability, resource, and error type,
  each labeled with what it requires from the language, compiler, runtime,
  stdlib, and SDK codegen.
- [10. Workflows](./pages/10-workflows.md) — process-surviving orchestration:
  durable steps, replay, signals, timers, child workflows, agents,
  provider-job tokens, executors, desugaring, and phased implementation.

Prior efforts this consolidates are recorded only in
[previous_work.md](./previous_work.md).

## Motivation

Most AI application code begins as a task with a contract:

```baml
class Invoice {
  vendor: string,
  total: float,
  currency: string,
}

function ExtractInvoice(document: pdf) -> Invoice {
  client: AccurateModel
  prompt: `Extract this invoice: ${document}
           ${ctx.output_format}`
}
```

The signature is the contract. The return type is the schema. The prompt is
inspectable. Tests, traces, and generated SDKs can name the task. This must
remain the shortest path, and this BEP does not change it.

The difficulty begins when execution stops being one immediate call: stream
partials to a UI; let the model call tools; run for an hour and poll; hold a
provider-stored conversation; open a live audio session; route across
providers with retry. These are different *lifecycles*, not different tasks.
A design that handles them by flags on one call invents invalid
combinations; a design that handles them by rewriting the prompt at each new
call site loses the schema, the roles, and the trace identity; a design that
generates a new function per (task × mode × extension) grows without bound.

The resolution: keep one task declaration, reify one invocation as a typed
value, and make each lifecycle a *consumer* of that value. The fixed modifier
set covers the standard lifecycles ergonomically; the request value covers
everything else.

## The five nouns

| Noun       | Meaning                                        | Example                              |
| ---------- | ---------------------------------------------- | ------------------------------------ |
| Task       | A named, typed AI task                         | `ExtractInvoice(document) -> Invoice`|
| Request    | One rendered invocation that has not run       | `ExtractInvoice.request(doc)`        |
| Provider   | A value that may implement capabilities        | `baml.ai.OpenAi { model: "..." }`    |
| Capability | One interaction shape a provider supports      | `Generate`, `Streaming`, `Background`|
| Resource   | A live/durable thing an operation returns      | `Job<Invoice>`, `Session`, `Live`    |

`Workflow` is intentionally not a sixth provider noun. It is an
orchestration declaration that composes the five nouns, and
`WorkflowRun<T>` follows the same resource/token discipline while being
owned by a workflow executor rather than an AI provider.

The flow every execution takes:

```text
task call or modifier
    -> baml.ai.Request<T>            (rendered prompt + output type + provider + options)
    -> a driver negotiates the provider's capabilities
    -> a capability method executes
    -> T, Stream<...>, Response<T>, Job<T>, Session, ...
```

## Specification overview

### Tasks

An LLM function is a task. Its body is declarative: `client`, `prompt`, and
optionally `tools` (page 5). The declared return type is simultaneously the
type of the plain call and the output schema rendered by
`${ctx.output_format}`. The `client` field names a default provider; every
task and every modifier accepts a `client =` override whose type is
`baml.ai.Provider`.

### Modifiers

The compiler gives every task a **closed** set of modifiers:

| Spelling                        | Returns                                       | Use when |
| ------------------------------- | --------------------------------------------- | -------- |
| `Extract(doc)`                  | `Invoice`                                     | you want the answer |
| `Extract.stream(doc)`           | `Stream<PartialInvoice, Invoice>`             | values should arrive incrementally |
| `Extract.with_meta(doc)`        | `baml.ai.Response<Invoice>`                   | you also need usage/finish/citations |
| `Extract.background(doc, opts?)`| `baml.ai.Job<Invoice>`                        | work outlives the call |
| `Extract.agent(doc, budget?)`   | `Done<Invoice> \| BudgetReached \| Handoff`   | the task has tools and you route on outcomes |
| `Extract.request(doc)`          | `baml.ai.Request<Invoice>`                    | anything else consumes the invocation |

(`PartialInvoice` denotes the compiler-derived partial form of `Invoice`;
its concrete spelling is out of scope for this BEP.)

Two tooling modifiers exist for inspection and offline parsing:
`Extract.prompt(doc) -> baml.llm.PromptAst` and
`Extract.parse(text) -> Invoice`.

The set does not grow. A user-defined execution mode is an ordinary function
over `Request<T>` (page 7); it is called as `mode(Extract.request(doc), ...)`
and needs no compiler support.

In generated SDKs, modifiers map to each language's established convention
(e.g. Python `extract_stream`, `extract__background`; TypeScript preserves
the member spelling).

### Requests

```baml
class Request<T> {
  provider: Provider,
  prompt: baml.llm.PromptAst,
  identity: TaskIdentity?,
  arguments: map<string, unknown>,     // captured call args, for traces (redaction: open q. 1)
  tools: Tool[],                       // the task-owned roster, if any
  options: RequestOptions,
  tags: map<string, string>,

  _render: PromptRenderRecipe,         // runtime-private; see below

  function messages(self) -> ChatMessage[] throws never
  function output_type(self) -> type throws never
  function provider_name(self) -> string throws never
  function for_provider(self, provider: Provider) -> Request<T> throws never
}
```

A request carries everything one invocation means: the selected provider,
the rendered structural prompt (roles and media intact), the output type `T`
(which rendered `${ctx.output_format}` and drives native schemas), the task's
stable identity for traces, the captured arguments, the task-owned tool
roster, and portable options. It carries **no** provider-specific wire body;
providers build their own wire requests from it.

A rendered `PromptAst` cannot be un-rendered, so the request privately
retains its **render recipe** — the template plus captured arguments —
purely to implement `for_provider`. Rebinding is a re-render, not a field
swap:

```baml
// request.for_provider(backup) is, conceptually:
Request<T> {
  ...request,
  provider: backup,
  prompt: request._render.render(backup, request.arguments, request.output_type()),
}
```

This is how fallback members and wrappers keep provider-sensitive prompt
context (`${ctx.client...}`, provider-specific formatting) correct per
attempt. `Request<T>` is **process-local and not serializable** — it holds a
provider value and a recipe closure; durable work crosses processes via
resource tokens (page 6), never via requests.

A request can also be built without a task, from a first-class lazy prompt
template:

```baml
let req = baml.ai.request<Invoice>(provider, prompt`
  Extract this invoice: ${document}
  ${ctx.output_format}
`)
```

The template has type `(baml.llm.Context) -> baml.llm.PromptAst` and renders
when the provider and `T` are known — the same laziness the task declaration
uses.

### Providers and capabilities

`Provider` is a marker interface with no interaction methods. Each
interaction shape is a capability interface that `requires Provider`. The
baseline is semantic generation:

```baml
interface Generate requires Provider {
  function generate<T>(self, request: Request<T>) -> Response<T>
    throws baml.errors.CallError | baml.errors.UnknownError
}

class Response<T> {
  value: T,
  meta: Meta,
}
```

An HTTP provider builds JSON and sends it; a local provider calls a host
runtime; a test provider returns a fixture; a policy wrapper delegates to an
inner `Generate`. Transport stages (request building, SSE decoding, SAP
parsing) are implementation helpers, not capabilities.

The standard capability families and their drivers:

| Family     | Capability   | Modifier / driver              | Result            |
| ---------- | ------------ | ------------------------------ | ----------------- |
| Immediate  | `Generate`   | plain call / `.with_meta`      | `T` / `Response<T>` |
| Incremental| `Streaming`  | `.stream`                      | `Stream<TPartial, T>` |
| Tool loop  | `Tools`      | `.agent`                       | outcome union     |
| Deferred   | `Background` | `.background`                  | `Job<T>`          |
| Batch      | `Batching`   | `baml.ai.submit_batch`         | `Batch<T>`        |
| Conversation | `Sessions` | `baml.ai.open_session`         | `Session`         |
| Realtime   | `Realtime`   | `baml.ai.open_live`            | `Live`            |
| Managed context | `ManagedCache` | `baml.ai.create_cache`  | `Cache`           |

There is no `Workflow` row in this table on purpose. Durability of arbitrary
application control flow is not an AI-provider interaction shape. Page 10
defines `baml.workflow` and its executor boundary separately.

### The three-layer surface rule

Where an operation is spelled depends on what you are holding:

1. **Holding a task** → use the task and its modifiers. This is almost all
   application code.
2. **Holding a concrete provider** → call capability methods directly:
   `m.generate<T>(req)`, `m.open_session(opts)`. The concrete type makes the
   capability statically known; no negotiation needed.
3. **Holding an existential `baml.ai.Provider`** (dynamic routing, combinator
   members, `request.provider` inside a driver) → use the `baml.ai.*` free
   functions, which perform the runtime capability `match` once, in one
   place.

Application authors live on layers 1–2 and hit layer 3 only when they choose
dynamic routing. Driver and wrapper authors live on layer 3; that is its job.

### Fluent sugar is an extension surface

`Provider` stays an empty marker. Dot-call conveniences such as
`.with_retry(...)` do not define provider semantics and do not belong to a
capability interface. The standard library supplies them through a separate
interface and an out-of-body blanket implementation:

```baml
interface ProviderFluent requires Provider {
  function with_retry(self, policy: RetryPolicy) -> Retry { ... }
  function fallback_to(self, other: Provider) -> Fallback { ... }
  function traced(self, meter: UsageMeter) -> Traced { ... }
}

implements<T extends Provider> ProviderFluent for T {}
```

This makes the methods available on every *concrete* provider, including
user-defined providers and concrete wrapper values, without reopening either
class or `Provider`. It intentionally does not add members to an existential
value whose static type is `Provider`. Layer 3 therefore remains explicit:

```baml
let concrete = baml.ai.OpenAi { ... }.with_retry(policy)

let routed: baml.ai.Provider = route_for(tenant)
let reliable = baml.ai.retry(routed, policy)
```

Libraries may define their own fluent interfaces and blanket implementations
for opinionated wrappers such as `.judged_by(...)`; they do not widen the core
marker or the standard fluent surface. Business decisions remain ordinary
functions. Page 8 defines the boundary and the rules for extension sugar.

### Resources

Operations whose follow-ups depend on provider-owned state return resource
objects, not bare identifiers:

```baml
let job = ExtractInvoice.background(doc, baml.ai.BackgroundOptions {
  idempotency_key: "invoice-9137",
})
defer { job.cleanup() }

match (job.poll()) {
  let done: baml.ai.Done<Invoice> => save(done.value),
  let pending: baml.ai.Pending => reschedule(job.token()),
  let failed: baml.ai.Failed => alert(failed),
}
```

A resource owns its remote identifier, its provider, its parser, and its
lifecycle (`poll`/`cancel`/`fork`/`close`/`cleanup` as appropriate). For
crossing process boundaries, `token()` returns a serializable non-secret
token and the owning provider exposes an explicit `resume_*` method. Scope-
bound cleanup composes with the language's `defer` and magic `cleanup()`.

### Dynamic clients

Providers are plain class values, so the language's struct-update spread
derives variants:

```baml
let Fast = baml.ai.OpenAi {
  model: "gpt-5",
  api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
}

// same credentials, headers, options — different model, this call only:
let invoice = ExtractInvoice(doc, client = baml.ai.OpenAi { ...Fast, model: "gpt-5-mini" })
```

Spread is same-class by construction, which is the correct constraint: an
`Anthropic` value has different fields than an `OpenAi` value, and pretending
otherwise is how option-blob designs go wrong. Page 4 covers the full
declaration-to-override spectrum, including task-level defaults built from
shared bases and function-valued clients for per-tenant routing.

### Custom capabilities

Extension requires no registration, no markers, and no compiler changes:

1. declare an `interface ... requires baml.ai.Provider`;
2. implement it on one or more providers;
3. write an ordinary generic driver over `Request<T>`;
4. call it with any task's `.request(...)`.

```baml
let note = run_moderated(ComposeNote.request(topic), "no-pii")
```

Page 7 is the end-to-end guide.

### Workflows

Ordinary BAML functions already compose tasks, custom providers, and
capabilities. When that orchestration must survive process death, the
proposed `workflow` declaration adds explicit durable effect boundaries:

```baml
workflow ProcessInvoice(input: InvoiceRef) -> Receipt {
  id: "billing.process_invoice"
  version: "1"

  let invoice = ctx.step(
    "extract",
    () -> Invoice { ExtractInvoice(input) },
  )

  ctx.step(
    "post",
    (activity: baml.workflow.ActivityContext) -> Receipt {
      post_invoice_to_ledger(invoice, activity.idempotency_key())
    },
  )
}
```

The executor, not the provider, owns the command journal, timers, signals,
workers, and replay. LLM functions remain the preferred model-call surface;
direct custom-provider calls remain available inside `ctx.step`. See
[page 10](./pages/10-workflows.md) for the end-to-end design and explicit
“available today vs proposed” boundary.

## Alternatives considered

Each page carries its own alternatives; these are the surface-level ones.

**One generated companion per (task × mode), open-ended.** Register execution
modes globally; the compiler synthesizes `Foo<mode>` for every task. Rejected:
the generated API grows as tasks × installed modes; mode names become
program-global (two libraries cannot both claim `moderated`); the compiler
must validate driver signature conventions; installing a library silently
changes every task's API; host SDKs must decide whether to emit third-party
members. The closed modifier set plus `.request` keeps growth bounded — a new
mode is a function, not a code-generation event.

**Free driver functions as the primary surface** (`baml.ai.run_with_meta(
Extract.request(doc))`). Semantically identical to `.with_meta`, and this BEP
keeps the free functions as the negotiation layer. Rejected as the *primary*
spelling: it inverts reading order — the mode comes first and the task is
buried inside — and it makes the most common discovery question ("how do I
stream this?") unanswerable by autocomplete on the task.

**Mode in the return type** (`function Solve(p) -> Response<Answer>`).
Rejected: the declared return type is the output schema; wrapping it forces
the compiler to special-case-unwrap envelope types everywhere schemas are
derived, and every call site pays `.value` for a property of *some* call
sites. Metadata is a property of a call, not of a task.

**A universal mode argument** (`Extract(doc, mode = background(...))`).
Rejected: mode return types differ (`T`, `Stream`, `Job<T>`, unions);
expressing one signature over them needs higher-kinded output types, and the
diagnostics would be unteachable.

**Methods on `Request<T>` instead of on the task**
(`Extract.request(doc).with_meta()`). Workable and considered at length; the
task-method form wins on ergonomics (one call, arguments in one place) and
on discoverability, and `.request` remains available so nothing is lost.
Request-methods may be added later without conflict; they are sugar over the
same free functions.

**Suffix-mangled companions** (`Extract$stream`, `Extract$background`).
Same synthesis, worse surface: sigils read as compiler internals, do not
autocomplete as members, and map awkwardly into host SDKs. Reserved-sigil
names remain for true internals (prompt rendering, parse plumbing) that
users should not reach for.

## Security and privacy

Requests capture arguments and rendered prompts; resource tokens name remote
state. Tracing and serialization MUST be opt-in at field granularity and
honor redaction. Tokens MUST NOT contain credentials; providers MUST validate
token ownership on resumption. Tool execution MUST preserve provider call
IDs, validate arguments against declared types, and gate side effects behind
explicit permission policy (page 5). Provider-specific raw metadata SHOULD be
redacted by default in production traces.

## Implementation notes

Phased narrowly; each phase is independently shippable and the difficult
contracts land before the features that depend on them:

1. Task companion selector resolution (page 3, "Selector semantics").
2. Complete `Request<T>` including the render recipe and provider-aware
   re-rendering.
3. Plain calls and `.request` lowered through requests; semantic `Generate`
   + `Response<T>`/`Meta`; built-in providers adapted (HTTP codec becomes
   provider-internal helpers).
4. `.with_meta` and `.stream` for tasks **without** tools.
5. `Background` and `Job<T>` (tokens, resume, cleanup).
6. Tools — only after the tools × modifier matrix (page 5) is normative;
   `tools:` field, `.agent`, outcome union.
7. Retry and fallback — only after `CommitState × ReplayPolicy` (page 8) is
   complete.
8. Sessions and resource capability refinements (page 6).
9. Custom-capability examples and docs once the seams above are stable.
10. Throughout: `client:` field grammar extended to provider expressions;
    SDK codegen for modifiers; `baml describe` presentation.
11. Workflows proceed as a separate follow-on track (page 10): generated
    BAML-as-activity adapters for host workflow engines first; native
    `workflow` syntax, checkpoint types, replay intrinsics, signals, and
    executor integrations in independently shippable phases.

Workflow implementation does not gate the provider-model acceptance criteria
below.

## Acceptance criteria

1. A task calls a built-in provider through `Generate` with no behavioral
   change to today's plain calls.
2. `.stream` produces the compiler-derived partial type from the same
   rendered request as the plain call.
3. `.with_meta` returns value + normalized metadata from one provider call.
4. `.background` returns a `Job<T>` that polls, cancels, serializes a token,
   resumes on a configured provider, and cleans up.
5. A task with a `tools:` field runs the loop on a plain call and exposes
   the outcome union via `.agent`.
6. A provider variant derived by spread (`{ ...Base, model: ... }`) works as
   a `client =` override with no other ceremony.
7. A user-authored provider implements `Generate` entirely in BAML.
8. A user-authored capability + driver runs against any task's `.request`
   with no compiler registration, and adding it creates no new members on
   any task.
9. Retry/fallback never re-drive an operation whose replay policy or error
   commit-state forbids it; refusal is a typed error.
10. `baml describe` lists a task's modifiers, its request signature, and the
    capability interfaces of its default client.

## Open questions

1. Should `Request.arguments` (captured call arguments) be public,
   trace-only, or behind a redacting accessor?
2. Should `Meta` expose provider-specific data as `attributes:
   map<string, unknown>` plus `raw: json?`, or as typed provider sidecars?
3. Which options are portable enough for `RequestOptions` (temperature?
   max output tokens?) versus provider fields?
4. Should the plain call be allowed to degrade `Generate` → drain
   `Streaming` when only the latter exists, and if so is that observable?
5. What stable provider-instance identifier do serialized resource tokens
   use?
6. Does `.agent` accept a call-site tool roster in addition to the task
   `tools:` field, and if both exist, do they merge or does the call site
   win?
7. Task modifiers on tasks used as values (BEP-062 function types): what is
   the type of bare `Extract`, and do modifiers ride it?
