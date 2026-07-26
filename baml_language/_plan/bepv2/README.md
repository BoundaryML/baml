> **Status:** Draft

# BEP-064: AI Functions and Agents

## Summary

BAML already makes one model call easy: declare an LLM function, give it a
typed return value, and call it like a normal function. Applications also need
streaming, tools, retries, background work, realtime sessions, and external
agent harnesses. Those features should not require a different kind of LLM
function for every lifecycle.

This proposal introduces one reusable boundary:

```text
LLM function -> Task -> Driver -> Provider capability -> typed outcome or resource
```

- An **LLM function** describes the work and its return type.
- A **Task** is one LLM-function call that has not run yet.
- A **Driver** controls how the task runs.
- A **Provider** performs the model-specific operation.
- A **Capability** says which operations a provider supports.
- A **Resource** owns live state that must be cleaned up.

The direct call remains the shortest path:

`SupportModel` below is an ordinary configured provider value. The user guide
shows one complete provider setup.

```baml
class Ticket {
  id: string,
  message: string,
}

class Resolution {
  category: string,
  reply: string,
}

function ResolveTicket(ticket: Ticket) -> Resolution {
  provider: SupportModel
  prompt: `
    Resolve this support ticket: ${ticket}
    ${ctx.output_format}
  `
}

let resolution = ResolveTicket(ticket)
```

The direct call returns only `Resolution`, which is right for most application
code. A production support service may also need the provider request ID for
debugging and token usage for billing. In that case, create the same call as a
task and choose the metadata driver:

```baml
let task = ResolveTicket.task(ticket)
let response = ai.drivers.drive_with_meta(task)

support_db.save_resolution(ticket.id, response.value)

match (response.meta.usage) {
  let usage: ai.Usage => billing.record_tokens(
    ticket.id,
    usage.input_tokens,
    usage.output_tokens,
  ),
  null => log.warn(`provider did not report usage for ${ticket.id}`),
}

let request_id = response.meta.request_id ?? "not reported"
log.info(`resolved ${ticket.id}; provider request ${request_id}`)
```

`support_db` and `billing` are ordinary application services; the `ai` API
only provides the typed value and its metadata.

`response.value` is still the typed `Resolution`. `response.meta` carries the
provider, model, request ID, finish reason, usage, and other diagnostic data.
If the caller does not need those details, it should use the direct
`ResolveTicket(ticket)` call.

Creating the task itself does not contact a model. The task keeps the selected
provider, prompt recipe, arguments, tools, output type, and options until
`drive_with_meta` consumes it.

## Motivation

Without a common task value, every new lifecycle tends to become new compiler
syntax or a generated method such as `.stream`, `.agent`, `.background`, or
`.realtime`. That approach has three problems:

1. The compiler must know every current and future execution style.
2. Third-party providers and drivers cannot compose through one stable API.
3. Ownership becomes unclear: prompt design, retry policy, provider protocol,
   and application side effects get mixed together.

BEP-064 keeps those responsibilities separate. The compiler only turns an LLM
function call into typed intent. Libraries define drivers. Providers implement
the capabilities they actually support. Applications retain control of
business state and tool side effects.

## Proposed Design

### LLM functions create tasks

Every LLM function has one generated companion: `.task(...)`.

```baml
ResolveTicket.task(ticket)
ResolveTicket.task(ticket, $provider = CarefulModel)
```

The direct call is defined in terms of that task:

```text
ResolveTicket(ticket)
  == ai.drivers.drive(ResolveTicket.task(ticket))
```

There are no lifecycle-specific generated companions. A new driver works with
every existing LLM function because each driver consumes `Task<T>`.

### Drivers own lifecycle policy

A driver is an ordinary typed function. It decides whether execution is a
single call, a stream, an agent loop, a background job, a batch, a session, or
another lifecycle.

```baml
let task = ResolveTicket.task(ticket)

let value = ai.drivers.drive(task)
let response = ai.drivers.drive_with_meta(task)
let stream = ai.drivers.stream(task)
let run = ai.drivers.run_agent(task, options)
```

The task's return type remains a promise across that lifecycle. A driver that
accepts `Task<T>` must make `T` observable on at least one successful terminal
path: directly, inside a response, through a deferred result, or in a terminal
outcome. An open-ended lifecycle with no single result accepts `Task<null>`
instead. For example, a realtime task supplies instructions and tools while
its `LiveSession` resource supplies events; it does not pretend that the session
produces one hidden `T`.

Safe drivers require the provider capability they use. An explicitly unsafe
driver may accept an erased provider and return `Unsupported` at runtime.

### Providers implement capabilities

`Provider` supplies common identity and prompt-rendering behavior. More useful
interfaces describe actual operations:

```text
DriveProvider          normal typed call
GenerationProvider     one model interaction
StreamingProvider      partial output stream
ToolCallingProvider    one tool-aware provider turn
BackgroundProvider     remote background job
SessionProvider        provider-owned session
RealtimeProvider       live duplex session
BatchProvider          remote or managed batch
```

A provider implements only the interfaces it can honor. For example, a model
adapter may support generation and streaming but not realtime. A custom
capability is another interface plus an ordinary driver function; it does not
need a compiler plugin.

### Tools keep execution ownership explicit

Application tools are typed handlers. The application validates and executes
them. Provider-owned tools such as hosted web search remain typed provider
configuration because the provider executes them.

The agent driver owns the loop: resolve the active tools, ask the provider for
one step, validate calls, apply policy hooks, dispatch application handlers,
submit one result for every call ID, enforce budgets, and either continue or
return a terminal outcome.

The driver keeps the mutable `ToolRegistry`. Before each provider turn,
`StepContext.tools` gives the hook a snapshot of the complete current roster;
the context does not expose the registry. `StepPlan.tools` is either `null` to
keep that roster or the complete replacement roster to use next. The driver
validates and applies a replacement before the next request, and the change
persists for later turns. A request already in flight keeps the tool schema it
was sent.

### Conversations and transcripts are different

`Conversation` is portable application-owned message history. Applications may
edit, compact, fork, store, and display it.

`Transcript` is provider-owned continuation state. It may contain reasoning
signatures, encrypted blocks, cache identifiers, or remote state that cannot be
reconstructed from visible messages. Switching providers requires an explicit
export/import operation that reports whether the conversion was exact,
message-only, or lossy.

### Stateful operations return resources

Background jobs, batches, sessions, realtime connections, caches, and harness
sessions return resource values. Every resource provides idempotent cleanup:

```baml
let live_session = VoiceSupport.task(customer_id).run(
  runner = ai.run.Realtime.new(channel),
)
```

`VoiceSupport` returns `null`: it describes how the live interaction should
behave, while the `LiveSession` resource exposes its many observable events and
controls.

`defer` is the deterministic production path. Runtime finalization is the
backup for unreachable resources, not the main lifecycle API.

### Events observe; hooks decide

Typed events describe calls, model output, tool execution, provider changes,
usage, and terminal outcomes. Observers and recorders do not mutate execution.
Hooks make explicit decisions such as blocking a tool call or changing the
next step's provider or tool roster.

This split keeps telemetry reusable and keeps behavioral policy visible.

### External harnesses consume tasks

A coding harness or another external agent runtime may own a longer loop and
its own session protocol. It consumes a `Task<T>` through the harness driver;
it is not automatically the task's model provider. Harness sessions, event
streams, steering, interruption, save/restore, and cleanup use the same
resource and ownership rules as provider-managed lifecycles.

## Design Tradeoffs

### One `.task` companion instead of many generated methods

This makes the driver call slightly more explicit, but it keeps lifecycle
vocabulary out of the compiler and lets user-defined drivers compose with
existing LLM functions immediately.

### Capability interfaces instead of one large provider interface

Callers must choose the operation they need, but providers no longer pretend
to support features they cannot implement. Static capability evidence catches
many invalid combinations before a request starts.

### Separate application and provider-owned tools

The model may see both kinds of tools in one request, but their execution and
security owners are different. Keeping them separate prevents an application
hook from accidentally changing a provider-managed capability.

### Separate conversations and transcripts

Two state types require more vocabulary, but a single message list cannot
honestly represent both editable application history and exact provider
continuation state.

### Resources require explicit cleanup

Cleanup adds one lifecycle obligation. In return, remote jobs, sockets,
sessions, caches, and child processes have a consistent owner and a safe
failure path.

## Open Questions

- What final constructor names should the public `ai.testing` namespace use
  for scripted outputs, tool turns, and classified failures?
- Should provider switching after a failed model turn have a standard hook, or
  remain a custom driver built from replay-policy primitives?
- Which persistent process transport should host runtimes expose for external
  JSONL or JSON-RPC harness adapters?
- Which providers should ship the first live `BatchProvider` adapters?

These questions do not change the task, driver, provider, capability, and
resource ownership model.

## Addenda

- [User guide](./pages/guide-overview.md): standalone usage recipes and a shared
  running example.
- [Specification](./pages/specification.md): normative behavior, ownership,
  and invariants.
- [API reference](./pages/specification/api-reference.md): proposed signatures
  in one place.
- [Rationale](./pages/rationale.md): previous work and design follow-ups.

Guide snippets use the proposed top-level `ai.*` API and contain the context a
reader needs. The private conformance package is useful to implementers, but it
is not required to understand or use this BEP.
