# Tasks, Runners, Providers, and Executable Tools

> **Status:** Direction under executable evaluation in
> `crates/baml_tests/baml_src_temp2`. This note records the intended ownership
> boundaries and naming. It does not yet replace the normative specification.

## Decision summary

An LLM function still lowers to a typed `Task<T>`. The task is run by a nominal
`Runner<Input>` value:

```baml
let task = ResolveTicket.task(ticket, provider = provider)

let outcome = task.run(
  runner = ai.run.Agent.new(
    tools = [lookup_account_tool],
    budget = ai.Budget { max_steps: 8, max_cost_usd: 0.25 },
  ),
)
```

The runner selects the lifecycle and therefore the result type. The provider
implements the backend capabilities required by that lifecycle:

```text
LLM function -> Task<T> -> Runner -> provider capability -> result/resource
```

The main names are:

| Previous experiment | Runner direction |
| --- | --- |
| `Driver<Input>` | `Runner<Input>` |
| `Task.drive(driver)` | `Task.run(runner = runner)` |
| `ai.driver.*` | `ai.run.*` |
| `DriveProvider.drive` | `CompletionProvider.complete` |
| `RunAgent<T>` + `AgentOptions` | `ai.run.Agent<T>` with its own fields |
| `Transcript` | provider-owned `Conversation` |
| editable `Conversation` | `MessageHistory` |

There are no default class fields in BAML. Runner factories use default
function arguments and return fully initialized class values.

## Core protocol

```baml
interface Runner<Input> {
  type Output
  type Error

  function run(self, input: Input) -> Self.Output throws Self.Error
}
```

`Task<T>` supplies the common unary convenience:

```baml
function run<
  Output,
  Error,
  R extends Runner<Task<T>, Output = Output, Error = Error>,
>(self, runner: R) -> Output throws Error {
  runner.run(self)
}
```

The parameter is deliberately named `runner`, producing a readable call site:

```baml
task.run(runner = ai.run.Stream.new())
```

Inputs that are not naturally owned by one task use the runner directly:

```baml
let batch = ai.run.Batch.new(provider).run(tasks)
let cache = ai.run.CreateCache.new(messages, ttl).run(provider)
let text = ai.run.Transcribe.new(provider).run(audio_stream)
```

## Responsibility model

### Task

`Task<T>` is an immutable description of one LLM-function invocation. It owns:

- function identity and declared result `T`;
- arguments and rendered prompt recipe;
- selected provider value;
- task-declared default application tools; and
- graph/debug metadata such as tags.

It does not own agent budgets, hooks, observers, a mutable registry, run IDs,
or continuation state. Those are execution-lifecycle concerns.

### Provider

A provider owns backend mechanics and exact provider state:

- model, endpoint, authentication, headers, and provider configuration;
- provider-specific request and response classes;
- prompt/wire rendering and native response decoding;
- one provider interaction for each capability method;
- provider-owned tools such as vendor web search or code execution;
- request IDs, encrypted reasoning blocks, cache handles, and continuation IDs;
- exact `Conversation` representation and ownership validation;
- request-level metadata and usage; and
- safe transport retries within one semantic interaction.

A provider advertises only the capabilities it can honestly execute:

```baml
interface CompletionProvider requires Provider {
  function complete<T>(self, task: Task<T>) -> Response<T>
}

interface GenerationProvider requires Provider {
  function generate<T>(self, task: Task<T>) -> Response<T>
}

interface ToolCallingProvider requires Provider {
  function begin<T>(self, task: Task<T>) -> Conversation
  function step<T>(self, conversation: Conversation, tools: Tool[]) -> ModelStep<T>
  function submit(self, conversation: Conversation, results: ToolResult[]) -> Conversation
}
```

`CompletionProvider` is bounded provider-default completion. It may be one
generation or a provider-managed loop, but it must return one `Response<T>` or
throw. `GenerationProvider` means exactly one model interaction.

### Runner

A runner owns lifecycle policy that is portable across capable providers:

- which provider capability is invoked;
- portable per-run configuration;
- loops and state transitions;
- application tool dispatch and parallelism;
- budgets, hooks, approvals, observers, and recorders;
- semantic retries, fallback, and provider switching;
- cancellation and resources opened by the runner; and
- the result shape exposed to the caller.

Some runners are intentionally thin capability adapters:

```baml
task.run(runner = ai.run.Completion.new()) // T
task.run(runner = ai.run.Background.new()) // Job<T>
task.run(runner = ai.run.Stream.new())     // Stream<TPartial, T>
```

Other runners are state machines. `ai.run.Agent` is the canonical
application-managed tool loop and returns:

```baml
Done<T> | BudgetReached | Handoff
```

Thin and substantial runners share a protocol because both choose the
execution lifecycle and associated output type, not because every runner must
contain the same amount of code.

## Why provider capabilities and runners both exist

A single provider can support several interaction shapes with incompatible
outputs:

```text
completion -> T
background -> Job<T>
streaming  -> Stream<TPartial, T>
agent      -> Done<T> | BudgetReached | Handoff
```

The provider itself cannot implement one unambiguous
`Runner<Task<T>>` mapping for all of them. Capability interfaces describe what
the backend can do; the runner value selects what the caller wants to do.

For background work specifically:

```baml
interface BackgroundProvider requires Provider {
  function submit<T>(self, task: Task<T>, options: BackgroundOptions) -> Job<T>
  function resume_job<T>(self, token: JobToken) -> Job<T>
}
```

`ai.run.Background` binds portable submission options and maps `Task<T>` to
`Job<T>`. Provider request encoding, job IDs, status mapping, polling, and
cancellation remain in the concrete `BackgroundProvider` and `Job<T>`.

The same capability can support another policy later:

```baml
task.run(runner = ai.run.Background.new())       // Job<T>
task.run(runner = ai.run.AwaitBackground.new())  // T
```

## Agent runner

`ai.run.Agent<T>` directly stores its lifecycle configuration. There is no
parallel `AgentOptions` data class:

```baml
class Agent<T> {
  budget: Budget?
  tools: Tool[]?
  tool_registry: ToolRegistry?
  hooks: AgentHooks?
  observers: AgentObserver[]
  recorders: AgentRecorder[]
  state: map<string, unknown>
  run_id: string?
  conversation: Conversation?
}
```

Its factory supplies defaults through function arguments. Its algorithm is:

```text
select conversation owner or task provider
resume conversation or provider.begin(task)
create or reuse the application tool registry

while true:
  enforce step and cost budgets
  apply prepare_step hook
  replace tools or import messages when switching provider
  provider.step(conversation, active_tools)
  accumulate usage and emit events

  final T:
    return Done<T>

  ToolCalls:
    detect handoff
    validate and authorize each call
    invoke executable tools, potentially in parallel
    preserve provider call IDs
    submit exactly one result per requested call
    conversation = provider.submit(conversation, results)
```

The Agent runner never contains OpenAI/Anthropic HTTP payload types, API keys,
native tool-block parsing, SAP parsing, or provider continuation internals.

## Provider-managed loops versus the Agent runner

The explicit Agent runner is preferred whenever BAML or the application owns
tool handlers, budgets, hooks, dynamic MCP discovery, provider switching, or
non-value terminal outcomes.

A provider may own the entire loop when the remote service genuinely executes
it—for example, a bounded vendor operation using only provider-owned search or
code-execution tools. From BAML this can be one
`CompletionProvider.complete` operation or a background `Job<T>`.

A provider composition may also package a fixed Agent policy as completion
convenience for an ordinary direct LLM-function call. That adapter must define
how `BudgetReached` and `Handoff` map to errors or predetermined actions. It is
secondary to the explicit runner because `CompletionProvider` cannot expose
the full terminal union.

An Agent runner depends on `ToolCallingProvider`, not `CompletionProvider`.
This prevents accidentally nesting a hidden provider completion loop inside an
application loop.

## Executable tools with `AnyFunction`

Application tools retain the original BAML function value:

```baml
class Tool {
  name: string
  description: string
  input_schema: json
  handler: baml.AnyFunction<
    Returns = unknown,
    Throws = baml.errors.ToolError,
  >
  handoff: bool
}
```

User code is concise and remains typed at definition time:

```baml
function lookup_account(
  customer_id: string,
  include_history: bool = false,
) -> Account throws baml.errors.ToolError {
  // ...
}

let tool = ai.tool(
  "lookup_account",
  "Look up a customer account.",
  lookup_account,
)
```

`reflect.signature(handler)` supplies parameter names, parameter types,
defaulted parameters, return type, error type, and documentation. `ai.tool`
derives an object schema in which defaulted arguments are optional.

The Agent runner dispatches with:

```baml
reflect.call_any(tool.handler, call.args)
```

The runtime checks the named arguments against the original function
signature and applies ordinary BAML default arguments. Arbitrary typed results
are serialized to `json` only at the provider boundary.

Closures and bound methods retain application state without putting secrets in
the model-visible schema. MCP tools are ordinary function-backed tools whose
handler closure captures the MCP connection. LLM functions can also be wrapped
explicitly as tools, keeping nested model calls visible in code and graphs.

Provider-owned tools remain provider configuration. The Agent runner does not
invoke them and they do not carry application function values.

## Conversation model

`MessageHistory` is editable and provider-neutral:

```baml
class MessageHistory implements Messages {
  entries: Message[]
}
```

`Conversation` is exact provider-owned continuation state:

```baml
interface Conversation {
  function provider(self) -> Provider
  function messages(self) -> Messages
}
```

Concrete values such as `OpenAiConversation` retain provider wire state while
exposing a portable message projection. The Agent runner owns which
conversation is active and accepts `conversation = resumed` when resuming.
`Task` does not carry continuation state.

Provider switching is explicit. A `ConversationImportProvider` imports
portable `Messages`, returns a new provider-owned conversation, and reports
`ConversationFidelity` plus warnings. Display names are never treated as
provider identity.

## Configuration placement

Use the narrowest honest owner:

- model, API key, vendor native tools, vendor request settings: provider;
- function arguments, prompt, default task tools: task;
- budgets, hooks, dynamic tools, observers, resume conversation: runner;
- polling/cancellation state: returned resource;
- captured application credentials: tool closure, never model-visible data.

Runner factories use named default function arguments. Provider factories use
the same technique for optional middleware or injected transports. Class
literals remain available when the caller intentionally supplies every field.

## Type-system requirements exposed by the experiment

The desired API should infer `Agent<T>` and other runner generics from
`Task<T>.run(runner = ...)`. Until outside-in inference is complete, explicit
type arguments may remain in the executable scenarios.

The current compiler can also expose `Task.run`'s generic `Error` instead of
the concrete runner error inside a helper with an explicit `throws` union.
Temp2 therefore lets hand-written default-call lowerings use the internal
completion function and lets affected helpers call a concrete runner's
`run(task)` method. This is an inference workaround, not the intended public
surface.

The final `Task<T, P>` design should preserve provider type `P` so incompatible
runner/provider combinations fail statically. The current temp2 compatibility
shape erases `P` and performs runtime capability matching; that is a known
prototype limitation, not the desired safety boundary.

Do not build a heterogeneous runtime runner registry for discoverability. It
would erase the associated input, output, and error types. Discovery belongs
in namespaces, documentation, LSP completion, and “find implementors” tooling.

## Invariants to test

The executable comparison must demonstrate:

1. Each standard runner preserves `T` in an observable terminal path.
2. Agent tools invoke their retained `AnyFunction` without a parallel dispatch
   switch.
3. Defaulted function arguments are optional in the derived schema and execute
   with their BAML default.
4. Closures and bound methods exclude captured state and bound `self` from the
   model-visible schema.
5. Tool rewrites preserve provider call IDs and cannot target an unregistered
   handler.
6. Every provider tool call receives exactly one submitted result, including
   blocked and invalid calls.
7. Resumption is configured on the runner and preserves provider ownership.
8. Provider switching uses explicit message import and reports fidelity.
9. Background and resource runners keep vendor wire logic in provider/resource
   implementations.
10. Live provider tests cover the same runner surface as offline fakes.
