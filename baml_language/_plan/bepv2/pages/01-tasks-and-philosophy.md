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
  client: AccurateModel
  prompt: `
    Extract this invoice: ${document}
    ${ctx.output_format}
  `
}

let invoice = ExtractInvoice(scan)                     // typed call
let cheap   = ExtractInvoice(scan, client = CheapModel) // model is swappable
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
libraries converge on task-first by folklore; BAML reifies the folklore and
checks it:

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

## The stability gradient

The second argument is about what churns. A task contract —
`ExtractInvoice(pdf) -> Invoice` — outlives everything around it. Models are
replaced constantly: renamed, retired, price-cut, leapfrogged. Model-first
puts the most volatile element at the center of the object graph, so model
churn ripples through application structure. Task-first makes model churn a
one-line default change or a `client =` at one call site:

```baml
// the day the new model ships, nothing else moves:
function ExtractInvoice(document: pdf) -> Invoice {
  client: NewerCheaperModel
  prompt: `...unchanged...`
}
```

Prompts are code; models are config. Put code in declarations and config in
fields.

## What a task gives you for free

Because the task is a declaration, the ecosystem can hang everything on it:

```baml
test "extracts the total" {
  let r = ExtractInvoice(fixture_pdf, client = FixtureProvider { reply: "..." })
  assert.equal(r.total, 1042.50)
}
```

- **Tests** name the task and swap the client for a deterministic fake.
- **Traces** carry the task identity, not an anonymous prompt string.
- **SDKs** generate a typed `extract_invoice(document)` in every host
  language.
- **Tooling** can render the prompt without executing
  (`ExtractInvoice.prompt(doc)`), and parse stored output without a network
  (`ExtractInvoice.parse(text)`).

## Where model-first is genuinely better — and what we do about it

Honesty requires naming the cases the task model fits less well.

**Chat-shaped work.** When the "prompt" is just the user's next utterance
plus history, a task wrapper is thin ceremony:

```baml
function Chat(messages: baml.ai.ChatMessage[]) -> AssistantTurn {
  client: ChatModel
  prompt: prompt_from_messages(messages)
}
```

It works, and it keeps typing and tracing, but the declaration adds little
over the message array. We accept this: the wrapper costs four lines once,
and everything downstream (sessions, page 6) still benefits from the typed
turn.

**Dynamic prompt construction.** Prompt optimizers, A/B harnesses, and
meta-programs want prompts as runtime data. The escape hatch is the lazy
template plus a manual request (page 2):

```baml
let req = baml.ai.request<Verdict>(judge_provider, prompt`
  ${role("system")} ${rubric_text}
  ${role("user")} ${candidate_answer}
  ${ctx.output_format}
`)
```

More ceremony than a Python f-string — deliberately. The ceremony is exactly
the information (output type, roles, provider binding) that ad-hoc strings
drop.

## When to call the task, and when to modify it

The rule of thumb for the rest of this guide:

- **You want the answer** → call the task: `ExtractInvoice(doc)`.
- **You want the answer to arrive differently** (streamed, deferred, with
  metadata, as an agent) → use a modifier: `ExtractInvoice.stream(doc)`,
  `.background(doc)`, `.with_meta(doc)`, `.agent(doc)`. Modifiers never
  change *what* the task is — only its execution lifecycle and,
  correspondingly, its return type (page 3).
- **You want an execution mode BAML does not ship** → take the invocation as
  a value, `ExtractInvoice.request(doc)`, and hand it to any function that
  consumes `baml.ai.Request<T>` (page 7).

A modifier changes the return type because lifecycles honestly differ: a
background run *is not* an `Invoice`, it is a `Job<Invoice>`. When you see a
modifier, read it as "same task, different contract with the caller."

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
task's `client:` field accepts it.

**Strings-first with schema inference** (infer the output type from usage).
Rejected: the schema drives prompt rendering (`${ctx.output_format}`), so it
must exist before the call; inference also makes the contract implicit
exactly where reviewers need it explicit.
