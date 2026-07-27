# Core types and inference

The design depends on preserving useful static types through task creation and
runner selection.

## Task

Conceptually:

```baml
class Task<T, P extends Provider = Provider> {
  function: baml.AnyFunction,
  arguments: json,
  provider: P,
  application_tools: Tool[],
}
```

The fields above describe runtime needs, not a required public layout.

`T` is the LLM function's declared result type. `P` is the selected provider
type. Neither is inferred from an erased field at the point of use; the
compiler carries both from the LLM function declaration.

`Task<T, P>` is immutable. Methods such as `with_provider`, `with_tools`, and
`with_tags` return a new task.

## Runner

The core protocol uses associated types:

```baml
interface Runner<Input> {
  type Output
  type Error

  function run(self, input: Input) -> Self.Output
    throws Self.Error
}
```

A concrete runner implementation can project both types from its input:

```baml
class Stream {}

implements<T, P> ai.Runner<ai.Task<T, P>> for Stream
  where P: ai.StreamingProvider
{
  type Output = ai.Stream<T>
  type Error = ai.StreamError

  function run(
    self,
    task: ai.Task<T, P>,
  ) -> ai.Stream<T> throws ai.StreamError {
    // ...
  }
}
```

The method-call form:

```baml
task.run(runner = runner)
```

must infer the same result and errors as:

```baml
runner.run(task)
```

## Why the runner is a value

Runner values keep configuration and state next to the lifecycle they affect:

```baml
ai.run.Agent.new(
  max_steps = 12,
  tools = [lookup_order],
)
```

They also provide one discoverable extension point: a user can look for
implementors of `Runner<Task<T, P>>`.

The interface does not require every runner to have the same shape. A batch
runner may accumulate submissions, a background runner may return a job
resource, and a stream runner may return immediately.

## Provider capabilities

Provider protocols are narrow interfaces:

```baml
interface Provider {
  function descriptor(self) -> ProviderDescriptor
}

interface CompletionProvider requires Provider {
  function complete<T>(
    self,
    task: Task<T, Self>,
  ) -> Response<T>
}

interface GenerationProvider requires Provider {
  function generate<T>(
    self,
    task: Task<T, Self>,
  ) -> Response<T>
}

interface StreamingProvider requires Provider {
  function stream<T>(
    self,
    task: Task<T, Self>,
  ) -> Stream<T>
}

interface ToolCallingProvider requires Provider {
  function begin<T>(
    self,
    task: Task<T, Self>,
  ) -> Conversation<Self>

  function step<T>(
    self,
    conversation: Conversation<Self>,
    tools: Tool[],
  ) -> ModelStep<T>

  function submit(
    self,
    conversation: Conversation<Self>,
    tool_results: ToolResult[],
  ) -> Conversation<Self>
}
```

These signatures are illustrative. The important rule is that a provider
implements operations it can actually perform. There is no all-capabilities
base interface with unsupported methods.

## Compile-time capability checks

Runner implementations express their provider requirements in their input
constraint:

| Runner | Required capability |
| --- | --- |
| `Completion` | `CompletionProvider` |
| `Generation` | `GenerationProvider` |
| `Stream` | `StreamingProvider` |
| `Agent` | `ToolCallingProvider` |
| `Background` | `BackgroundProvider` |
| `Batch` | `BatchProvider` |

The following must fail during type checking, not after a request starts:

```baml
let task: ai.Task<Resolution, TextOnlyProvider> =
  ResolveTicket.task(ticket)

task.run(runner = ai.run.Agent.new())
```

The diagnostic should say:

```text
ai.run.Agent requires ToolCallingProvider,
but TextOnlyProvider does not implement it.
```

## Required inference

The type checker must infer all of these without explicit type arguments:

```baml
let task = ResolveTicket.task(ticket)
// Task<Resolution, SupportModel>

let value = task.run(runner = ai.run.Completion.new())
// Resolution

let response = task.run(runner = ai.run.Generation.new())
// Response<Resolution>

let outcome = task.run(runner = ai.run.Agent.new())
// AgentOutcome<Resolution>
```

No user-facing signature in this path should display `<unknown>`.

Inference must work through:

- method syntax;
- generic interface constraints;
- associated type projection;
- generic `implements` blocks;
- finite provider unions;
- named default arguments on runner constructors; and
- wrapper runners such as `Retry<Inner>` and `Fallback<Inner>`.

## Variance and batch work

`Task<T, P>` is not generally declared covariant merely to make heterogeneous
arrays convenient. A task or runner may retain type-dependent state, and user
implementations can make variance assumptions unsafe.

The basic batch API is homogeneous:

```baml
ai.run.Batch<Resolution>.new(...)
```

A heterogeneous queue uses typed submission handles:

```baml
let queue = ai.BatchQueue.new(provider = BatchModel)

let ticket_job: ai.BatchItem<Resolution> =
  queue.add(ResolveTicket.task(ticket))

let summary_job: ai.BatchItem<Summary> =
  queue.add(Summarize.task(document))

let batch = queue.submit()
```

The queue may erase task internals after registration, but each returned
`BatchItem<T>` retains its own result type. A heterogeneous registry must not
erase the associated types at the public read boundary.
