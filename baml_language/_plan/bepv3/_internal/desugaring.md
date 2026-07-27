# LLM function lowering

An LLM function has two related meanings:

1. it is callable like a function; and
2. it can construct a typed description of that same call.

The compiler must preserve both meanings without duplicating the prompt,
arguments, provider selection, or tool declaration.

## Source form

```baml
function ResolveTicket(ticket: Ticket) -> Resolution {
  provider: SupportModel

  prompt: `
    Resolve this support ticket.

    ${ticket}

    ${ctx.output_format}
  `

  tools: [
    lookup_order,
    search_policy,
  ]
}
```

The public companion operation is:

```baml
let task: ai.Task<Resolution, SupportModel> =
  ResolveTicket.task(ticket)
```

The direct call is:

```baml
let resolution: Resolution = ResolveTicket(ticket)
```

## Conceptual lowering

The compiler may lower the declaration into any representation that preserves
the following conceptual pieces:

```baml
// Pseudocode. These are not user-visible declarations.
let ResolveTicket_function: baml.AnyFunction

function ResolveTicket_task(
  ticket: Ticket,
) -> ai.Task<Resolution, SupportModel> {
  ai.internal.Task.create(
    function = ResolveTicket_function,
    arguments = { ticket: ticket },
    provider = SupportModel,
    application_tools = ai.internal.normalize_tools([
      lookup_order,
      search_policy,
    ]),
  )
}

function ResolveTicket(ticket: Ticket) -> Resolution {
  ai.internal.run_direct(ResolveTicket_task(ticket))
}
```

`normalize_tools` accepts plain functions and configured `ai.Tool` values.
Plain functions coerce to `baml.AnyFunction`; BAML derives their name,
documentation, schema, handler, and default policy. `baml.AnyFunction` is the
heterogeneous callable identity used inside tool metadata, schemas, traces,
and registries. It must not erase the task's public return type. The
surrounding `Task<Resolution, SupportModel>` retains that type.

The task stores enough information to render the prompt later. Constructing it
does not render a provider request, perform I/O, or consume a budget.

## `.task(...)`

For every LLM function `F(A...) -> T`, the compiler exposes a companion:

```baml
F.task(A...) -> ai.Task<T, P>
```

`P` is the statically selected provider type when it is known. If provider
selection is dynamic, `P` may be an interface or a finite union, but it must
not become `unknown` merely because the call is represented as a task.

The task includes:

- the semantic identity of `F`;
- captured, typed arguments;
- the prompt and output-format recipe;
- the selected provider value;
- the default application tools;
- function and call-site tags; and
- the declared output schema.

The task does not contain:

- an already-rendered vendor request;
- a live provider conversation;
- a retry counter;
- a stream;
- a background job; or
- runner configuration.

Those belong to execution.

## `.run(...)`

`Task.run` is generic over the concrete runner:

```baml
// Conceptual signature.
function Task.run<R>(
  self: Task<T, P>,
  runner: R,
) -> R.Output
  throws R.Error
  where R: ai.Runner<Task<T, P>>
```

The call delegates once:

```baml
runner.run(self)
```

It must not recursively call `Task.run`. The compiler should resolve the
interface implementation for the concrete `R` and emit a virtual call to that
implementation.

Associated `Output` and `Error` types are projected from the selected runner.
This is what makes these expressions have different static types:

```baml
let value: Resolution = task.run(
  runner = ai.run.Completion.new(),
)

let stream: ai.Stream<Resolution> = task.run(
  runner = ai.run.Stream.new(),
)

let outcome: ai.AgentOutcome<Resolution> = task.run(
  runner = ai.run.Agent.new(),
)
```

## Direct calls

A direct call lowers through the same task constructor:

```baml
ResolveTicket(ticket)

// Conceptually:
ai.internal.run_direct(ResolveTicket.task(ticket))
```

`run_direct` is selected by the compiler or standard runtime. It is not a
provider method.

If the task has no application tools, `run_direct` uses the standard bounded
completion lifecycle. If it has application tools, `run_direct` uses the
standard BAML Agent lifecycle and collapses only `Done<T>` to `T`.

The exact behavior is specified in
[Direct calls and loop ownership](./direct-calls-and-loop-ownership.md).

## Overrides

Task-building overrides create a new task value. They do not mutate an LLM
function declaration:

```baml
let task = ResolveTicket
  .task(ticket)
  .with_provider(FallbackModel)
  .with_tools([lookup_order])
```

The result keeps the same semantic function identity and output type. An
override that changes provider type changes the task's provider type:

```text
Task<Resolution, SupportModel>
    .with_provider(FallbackModel)
→ Task<Resolution, FallbackModel>
```

The compiler must re-check runner capability constraints against the resulting
provider type.

## Graph identity

All runner paths must attribute provider work to the originating LLM function.
Graphs may insert runner and tool nodes, but they must not replace the LLM
function with a generic anonymous `run` call.

Conceptually:

```text
ResolveTicket
  └─ ai.run.Agent
       ├─ SupportModel step
       ├─ lookup_order
       └─ SupportModel step
```

This identity requirement applies to direct calls, explicit runners,
background jobs, batches, retries, streams, and external harness adapters.
