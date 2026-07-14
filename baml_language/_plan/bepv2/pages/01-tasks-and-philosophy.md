# 1. Tasks and Philosophy

This page explains the single decision everything else in BEP-064 follows
from: **BAML makes the task primary and the model secondary.** If you
understand why, the rest of the design is predictable.

## Two ways to organize AI code

Every LLM library must answer one structural question: what is the thing you
declare once and reuse — the *model* or the *task*?

**Model-first** libraries make the model primary. You configure an object
around a model, and the task arrives at runtime as a string. Pydantic AI is
the clearest example, and a good one — it is a well-designed library:

```python
from pydantic_ai import Agent, RunContext

agent = Agent(
    'openai:gpt-5',
    output_type=Invoice,             # schema is a constructor argument
    system_prompt='You extract invoices.',
    deps_type=Deps,
)

@agent.tool
def lookup_vendor(ctx: RunContext[Deps], vendor_id: str) -> str:
    return ctx.deps.db.vendor(vendor_id)

result = agent.run_sync(f"Extract this invoice: {doc}")   # task = runtime string
invoice = result.output
```

**Task-first** is what BAML does. You declare the task — a typed signature
whose implementation is a prompt — and the model is a field with a default:

```baml
class Invoice {
  vendor: string,
  total: float,
  currency: string,
}

function ExtractInvoice(document: pdf) -> Invoice {
  provider: AccurateModel
  prompt: `
    Extract this invoice: ${document}
    ${ctx.output_format}
  `
}

let invoice = ExtractInvoice(scan)                     // typed call
let cheap   = ExtractInvoice(scan, $provider = CheapModel) // model is swappable
```

## The wrapper argument

Watch what each design forces its users to write by hand, because that is
the tell for which abstraction should have been primary.

In every production model-first codebase, this function appears within a
week:

```python
def extract_invoice(doc: str) -> Invoice:
    return invoice_agent.run_sync(f"Extract this invoice: {doc}").output
```

A typed signature, a prompt hidden behind it, a model buried inside. That
hand-written wrapper *is* the BAML task — except the interpolation is an
unchecked f-string, the signature is maintained by hand, and nothing ties the
`Invoice` annotation to what the agent actually parses. Users of model-first
libraries converge on task-first by folklore; BAML turns that pattern into a
checked language feature:

- the template may only reference declared parameters (`${document}` is
  checked against the signature);
- the return type **is** the parse schema — `${ctx.output_format}` renders
  from it, and the response is parsed into it;
- the task has a stable name that tests, traces, `baml describe`, and
  generated SDKs all share.

The reverse recovery does not work. A BAML program that needs a model-first
object gets one cheaply — providers are values, tool bundles are values
(page 5), and a lazy prompt template is a first-class expression (page 2).
A Python program that needs a checked task gets an unchecked wrapper. Make
primary the abstraction whose inverse cannot be recovered safely.

## Related designs — and why this exact combination is rare

This proposal should not claim that no other framework has explored these
ingredients. Several established systems contain close precedents:

- [Vercel AI SDK](https://ai-sdk.dev/docs/reference/ai-sdk-core/agent) has a
  common `Agent` interface whose implementations may encapsulate tool use and
  multi-step workflows. Its
  [`ToolLoopAgent.generate`](https://ai-sdk.dev/docs/reference/ai-sdk-core/tool-loop-agent)
  runs an agent to completion, while `generateText` represents the lower-level
  model-oriented function. The callable abstraction is the configured agent
  object, not an independently declared typed task function.
- [Pydantic AI](https://pydantic.dev/docs/ai/core-concepts/agent/) makes
  `Agent` the primary typed object. It bundles instructions, tools, output
  type, dependencies, and an optional default model; `agent.run(...)` drives
  the graph to a typed final output. The prompt/task arrives at run time.
- [LangChain](https://docs.langchain.com/oss/python/langchain/models) gives
  models direct `invoke`/`stream` operations and
  [agents](https://docs.langchain.com/oss/python/langchain/agents) their own
  loop-owning `invoke` operation. The call target is a model, runnable, agent,
  or graph; the task is normally messages or state supplied at invocation.
- [DSPy](https://dspy.ai/) makes typed Signatures task-like, then selects
  execution by constructing a Module such as a predictor or ReAct module. The
  Signature itself is not both a directly executable function and something
  that can also produce a provider-bound task value.

The unusual part of this BEP is the combination:

```baml
MyFunction(args, $provider = P)       // execute P's default drive -> T
MyFunction.task(args, $provider = P)  // create Task<T, P>, no I/O
```

Most host-language libraries cannot express that dual surface without
decorators, proxies, macros, or generated wrappers. They must preserve the
function's parameter types, return schema, prompt recipe, stable identity,
provider type, and companion operation simultaneously. BAML owns the parser,
type checker, lowering, runtime, and SDK generation, so both forms can be two
checked views of one declaration rather than conventions users keep aligned.

There are four further reasons the design is uncommon.

### Frameworks usually start model-first or agent-first

Most frameworks historically grew from an HTTP/model wrapper into tool calls,
then agent objects and workflow runtimes:

```text
HTTP wrapper -> model -> tools -> agent -> workflow
```

Their stable declaration is therefore the model or agent, and the task remains
run-time prompt data. BAML starts with a typed LLM function, so it can instead
grow in the opposite direction:

```text
typed LLM function -> Task<T, P> -> provider-defined drive or explicit driver
```

Retrofitting this onto a mature model-first API would invert its existing
object model and compatibility contract.

### Providers traditionally stop at transport

Most libraries ask a provider adapter, “how do I call this vendor?” This BEP's
`DriveProvider` additionally asks, “when this provider value is selected for a
direct LLM-function call, how does it complete the task?” A basic provider may
answer with one call through `GenerationProvider`; an `Agent` composition may
answer with a whole tool loop. That is a deliberately broader contract than a
wire adapter.

### The type relationship is difficult in host languages

The direct form requires `$provider: DriveProvider` because it promises `T`.
The task form accepts `$provider: Provider`, retains its concrete type as
`Task<T, P>`, and lets an explicit driver demand `StreamingProvider`,
`ToolCallingProvider`, `BackgroundProvider`, or another capability. Dynamic
routing may intentionally erase `P` and use an `unsafe` runtime-negotiated
driver. Keeping all three cases both ergonomic and sound is substantially
easier in BAML's compiler than in erased Python generics or structurally
inferred TypeScript wrappers.

For example, the BAML call checker can apply one rule to every LLM function:

```baml
// Direct call: the selected provider must implement DriveProvider.
let invoice = ExtractInvoice(doc, $provider = InvoiceAgent)

// Task value: retain the concrete provider type.
let task = ExtractInvoice.task(doc, $provider = FastOpenAi)
let static_stream = ai.drivers.stream(task) // statically allowed because
                                                 // FastOpenAi: StreamingProvider

// Intentional erasure: static capability evidence is gone.
let selected: Provider = route_for(tenant)
let dynamic = ExtractInvoice.task(doc, $provider = selected)
// ai.drivers.stream(dynamic)         // compile error
let dynamic_stream = ai.drivers.unsafe.stream(dynamic) // checked at runtime
```

TypeScript can represent the same contract, but a library or code generator
needs a callable object with separate overloads for the default provider and
an override, plus corresponding overloads for `.task`:

```typescript
declare const providerBrand: unique symbol;

interface Provider {
  readonly [providerBrand]: true; // prevent accidental structural matches
}

interface DriveProvider extends Provider {
  drive<T>(task: Task<T, Provider>): Promise<Response<T>>;
}

interface StreamingProvider extends Provider {
  stream<T>(task: Task<T, Provider>): AsyncIterable<T>;
}

interface ExtractInvoiceFunction {
  // Direct form: omitted default and explicit override are distinct overloads.
  (document: Pdf): Promise<Invoice>;
  <P extends DriveProvider>(
    document: Pdf,
    options: { $provider: P },
  ): Promise<Invoice>;

  // Task form: the provider type must survive in the result.
  // `typeof` here is TypeScript's value-to-type operator, not BAML syntax.
  task(document: Pdf): Task<Invoice, typeof AccurateModel>;
  task<P extends Provider>(
    document: Pdf,
    options: { $provider: P },
  ): Task<Invoice, P>;
}

declare const ExtractInvoice: ExtractInvoiceFunction;
declare function stream<T, P extends StreamingProvider>(
  task: Task<T, P>,
): AsyncIterable<T>;
declare function unsafeStream<T>(
  task: Task<T, Provider>,
): AsyncIterable<T>; // performs a runtime capability check

const routed: Provider = routeFor(tenant);
const dynamic = ExtractInvoice.task(document, { $provider: routed });
// stream(dynamic);       // compile error: Provider is not StreamingProvider
unsafeStream(dynamic);    // runtime type guard or Unsupported error
```

If routing produces only `Provider`, TypeScript also needs a runtime type guard
before streaming, just as BAML's `drivers.unsafe.stream` does. This is not
impossible in TypeScript; it is overload, branding, and narrowing machinery
that must be generated consistently for every function. In BAML, LLM
functions are a compiler-known declaration kind, so the compiler can preserve
`P`, enforce the narrower direct-call bound, and make erasure explicit without
asking each provider library to recreate that machinery.

### A provider override can change effects

This is the design's main semantic risk:

```baml
ExtractInvoice(doc, $provider = OneTurnModel)  // perhaps one request
ExtractInvoice(doc, $provider = InvoiceAgent)  // perhaps many turns and tools
```

The second spelling may cost more, take longer, and execute application side
effects even though the LLM function's result type is unchanged. Other
frameworks often make the model/agent distinction syntactically explicit to
keep that change visible.

This BEP accepts the power only with three invariants:

1. A direct LLM-function call always returns its declared `T` or throws. It
   never widens to lifecycle outcomes.
2. The selected provider's `DriveProvider` implementation, replay policy, tool/effect
   behavior, and capabilities must be visible through `baml describe`, the
   LSP, and traces.
3. Code that needs explicit lifecycle outcomes, event streams, budgets,
   handoffs, approval policy, or effect control uses `.task(...)` plus an
   explicit driver.

The novelty is therefore not “agents can run to completion” or “tasks can be
typed.” Other frameworks do both. It is making a compiler-declared task
function simultaneously callable, convertible into a task value,
provider-overridable, and open to a provider-defined default execution strategy
without losing its typed contract.

## The stability gradient

The second argument is about what churns. A task contract —
`ExtractInvoice(pdf) -> Invoice` — outlives everything around it. Models are
replaced constantly: renamed, retired, price-cut, leapfrogged. Model-first
puts the most volatile element at the center of the object graph, so model
churn ripples through application structure. Task-first makes model churn a
one-line default change or a `$provider =` at one call site:

```baml
// the day the new model ships, nothing else moves:
function ExtractInvoice(document: pdf) -> Invoice {
  provider: NewerCheaperModel
  prompt: `...unchanged...`
}
```

Prompts are code; models are config. Put code in declarations and config in
fields.

## What a task gives you for free

Because the task is a declaration, the ecosystem can hang everything on it:

```baml
test "extracts the total" {
  let r = ExtractInvoice(fixture_pdf, $provider = FakeProvider { reply: "..." })
  assert.equal(r.total, 1042.50)
}
```

- **Tests** name the task and swap the provider for a deterministic fake.
- **Traces** carry the task identity, not an anonymous prompt string.
- **SDKs** generate a typed `extract_invoice(document)` in every host
  language.
- **Tooling** can render the prompt without executing
  (`ai.inspect.prompt(ExtractInvoice.task(doc))`), and parse stored output
  without a network (`ai.parse(ExtractInvoice.task(doc), text)`).

## Where model-first is genuinely better — and what we do about it

Honesty requires naming the cases the task model fits less well.

**Chat-shaped work.** When the "prompt" is just the user's next utterance
plus history, a task wrapper is thin ceremony:

```baml
function Chat(messages: ai.ChatMessage[]) -> AssistantTurn {
  provider: ChatModel
  prompt: prompt_from_messages(messages)
}
```

It works, and it keeps typing and tracing, but the declaration adds little
over the message array. We accept this: the wrapper costs four lines once,
and everything downstream (sessions, page 6) still benefits from the typed
turn.

**Dynamic prompt construction.** Prompt optimizers, A/B harnesses, and
meta-programs want prompts as runtime data. The escape hatch is the lazy
template plus a manual task (page 2):

```baml
let task = ai.task<Verdict>(judge_provider, prompt`
  ${role("system")} ${rubric_text}
  ${role("user")} ${candidate_answer}
  ${ctx.output_format}
`)
```

More ceremony than a Python f-string — deliberately. The ceremony is exactly
the information (output type, roles, provider binding) that ad-hoc strings
drop.

## When to call the function, and when to select a driver

The rule of thumb for the rest of this guide:

- **You want the answer** → call the LLM function: `ExtractInvoice(doc)`.
- **You want a different lifecycle** → create a task value once and select a
  driver: `ai.drivers.stream(ExtractInvoice.task(doc))`,
  `submit_background(...)`, `drive_with_meta(...)`, or `run_agent(...)`.
- **You want an execution mode BAML does not ship** → write an ordinary
  function consuming `ai.Task<T>` (page 7).

A driver changes the return type because lifecycles honestly differ: a
background run *is not* an `Invoice`, it is a `Job<Invoice>`. The driver name
makes that caller contract explicit without generating a member per lifecycle.

## Alternatives considered

**Make the agent object primary and generate task wrappers.** (The
model-first design with codegen on top.) Rejected: the wrapper generator
needs exactly the information a task declaration contains — signature,
prompt, schema — so the declaration is the source of truth either way, and
making the object primary reintroduces the churn coupling above.

**Make both primary (a task *and* a required agent declaration).** Rejected:
in the common case the "agent" is just `{model}`; forcing two declarations
per task doubles ceremony for nothing. Where an agent-like bundle is real
(model + tools + policy), it exists as a provider value (page 5), and the
task's `provider:` field accepts it.

**Strings-first with schema inference** (infer the output type from usage).
Rejected: the schema drives prompt rendering (`${ctx.output_format}`), so it
must exist before the call; inference also makes the contract implicit
exactly where reviewers need it explicit.
