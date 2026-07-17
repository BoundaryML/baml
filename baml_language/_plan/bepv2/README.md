> **Status:** DRAFT — design review. The executable reference package compiles;
> syntax/runtime gaps are tracked in [v2_deviations.md](./v2_deviations.md).

# BEP-064: AI Functions and Agents

## Abstract

BAML should make it obvious when code intends to use an AI model, keep LLM
functions as the preferred typed surface, and still provide complete escape
hatches without encouraging raw vendor `fetch` calls.

AI-specific standard-library APIs live in the top-level `ai` namespace,
parallel to top-level namespaces such as `baml` and `assert`. They are not
nested under `baml`.

The design has one compiler boundary:

```baml
MyLLMFunction.task(args..., $provider = OptionalOverride) -> ai.Task<T, P>
```

That is the only public companion generated for an LLM function. The LLM
function itself remains directly callable. A direct call asks the selected
provider to perform the default `drive` behavior defined by `DriveProvider`:

```baml
MyLLMFunction(args..., $provider = SelectedProvider)
// lowers to
ai.drivers.drive(
  MyLLMFunction.task(args..., $provider = SelectedProvider),
)
```

Everything else—streaming, metadata, agent loops, background jobs, sessions,
realtime, media generation, custom harnesses—is an ordinary library driver
that consumes a task. Providers implement capability interfaces. Stateful
operations return resources.

## Goals

- An LLM function remains the shortest, most legible way to declare typed AI
  work: arguments, return schema, prompt, default provider, and optional tools.
- The compiler generates only `.task(...)`; it does not own an ever-growing
  lifecycle vocabulary.
- Standard drivers cover common interaction shapes out of the box.
- Safe drivers require the relevant capability statically when possible.
  `drivers.unsafe.*` explicitly opts into runtime capability negotiation.
- Providers can be swapped per call and during an agent loop.
- Tools can come from the task, provider, driver options, MCP server, or a
  per-step hook, including halfway through a loop.
- Messages and transcripts are interfaces. Applications can inspect and
  serialize neutral conversations without being asked to preserve undocumented
  provider continuation state.
- Users can write custom providers, capabilities, drivers, wrappers,
  transcript importers, and fluent sugar in BAML using out-of-body
  `implements`.
- There is effectively zero incentive to hand-write raw vendor HTTP requests;
  concrete provider capabilities remain the lowest supported escape hatch.

## Non-goals

- Desugaring implementation in this phase. The scenario suite may continue to
  contain manually written reference expansions until the contract settles.
- Pretending every provider supports every lifecycle.
- Making provider-neutral conversation data an exact representation of
  provider-private continuation state.
- Folding durable workflow scheduling into model providers.

## Reading guide

- [1. Tasks and philosophy](./pages/01-tasks-and-philosophy.md) — task-first
  motivation, adjacent framework designs, and why the direct-call/task-value
  dual surface is unusual
- [2. Desugaring](./pages/02-desugaring.md) — sole `.task` companion and plain-call lowering
- [3. Drivers](./pages/03-drivers.md) — initial catalog, safe/unsafe layers, hooks
- [4. Providers and capabilities](./pages/04-providers-and-capabilities.md)
- [5. Tools, messages, transcripts, and agents](./pages/05-tools-and-agents.md)
- [6. Resources](./pages/06-resources.md)
- [7. Custom capabilities](./pages/07-custom-capabilities.md)
- [8. Reliability and errors](./pages/08-reliability-and-errors.md)
- [9. Normative signatures](./pages/09-normative-signatures.md)
- [11. Tool calling after BEP-062](./pages/11-tool-calling-after-bep-062.md) —
  function-backed tools, reflection-based dispatch, and the remaining runtime
  tool boundary
- [User guide](./user-guide/README.md) — progressive, usage-first examples and
  themed recipes built around one running application
- [Executable-reference deviations](./v2_deviations.md) — exact differences
  between the normative design and `crates/baml_tests/baml_src_temp`
- [Reconciliation task list](./reconciliation_task_list.md) — decisions,
  implementation work, and executable verification gates

Prior efforts are recorded in [previous_work.md](./previous_work.md).

## The six nouns

| Noun | Meaning | Example |
| --- | --- | --- |
| LLM function | Named typed declaration of AI intent | `ExtractInvoice(pdf) -> Invoice` |
| Task | One invocation that has not run | `ExtractInvoice.task(scan)` |
| Driver | Execution/lifecycle policy over a task | `drivers.run_agent(task)` |
| Provider | Value implementing zero or more capabilities | `OpenAi { model: "..." }` |
| Capability | One supported interaction shape | `DriveProvider`, `GenerationProvider`, `ToolCallingProvider`, `RealtimeProvider` |
| Resource | Provider/harness-owned live state | `Job<T>`, `Session`, `Live` |

A workflow is durable orchestration above these six nouns.

## Core flow

```text
LLM function declaration
  -> MyFunction.task(args, $provider = default)    compiler-owned, no I/O
  -> ai.drivers.<lifecycle>(task)             execution policy
  -> provider capability                           semantic adapter boundary
  -> provider wire protocol                        provider-private
  -> typed result / event stream / resource
```

## Task declaration and desugaring

```baml
class Invoice {
  vendor: string,
  total: float,
}

function ExtractInvoice(document: pdf) -> Invoice {
  provider: AccurateModel
  tools: [lookup_vendor]
  prompt: `Extract this invoice: ${document}. ${ctx.output_format}`
}
```

Let `P_default` mean the static type the compiler infers for `AccurateModel`,
and let `P` mean the static type inferred for an explicit override expression.
These are compiler typing rules, not BAML type aliases:

```text
ExtractInvoice.task(document)
  -> Task<Invoice, P_default>

ExtractInvoice.task(document, $provider = override)
  -> Task<Invoice, P>

ExtractInvoice(document)
  -> Invoice                         requires P_default: DriveProvider

ExtractInvoice(document, $provider = override)
  -> Invoice                         requires P: DriveProvider
```

BAML has no `typeof(value)` type operator. The compiler already knows the
static type of the declared provider and override expressions, so it carries
that type into `Task<Invoice, P>` directly. `reflect.type_of<T>()` is different:
it returns a runtime `type` value after `T` is known, which tasks and providers
use for output schemas and parsing.

`.task(...)` accepts any `Provider`; the eventual explicit driver supplies its
own capability constraint.

`$provider` is reserved and also stored on the task. It is conspicuously not a
domain argument. `client:` and `client =` may remain compatibility aliases, but
canonical BEPv2 terminology is provider.

```baml
let task = ExtractInvoice.task(scan, $provider = CheapModel)
let invoice = ai.drivers.drive(task)

// same result through the directly callable LLM function:
let invoice = ExtractInvoice(scan, $provider = CheapModel)
```

`Task<T, P>` contains the provider, structural prompt, output type, task
identity, captured arguments, default tools, options/tags, optional transcript,
and a private render recipe. `P` retains the provider's static capability
evidence; `Task<T>` is the intentionally erased shorthand.
`task.with_provider(next)` re-renders; it never merely changes a field.

## Direct calls and provider-default drive

Every provider usable as an LLM function default implements `DriveProvider`:

```baml
interface DriveProvider requires Provider {
  function drive<T>(self, task: Task<T>) -> Response<T>
  function replay_policy<T>(self, task: Task<T>) -> ReplayPolicy {
    ReplayPolicy { kind: ReplayKind.Never, idempotency_key: null }
  }
}
```

`DriveProvider` is the provider's high-level answer to “if this task is called
directly, how do I complete it?” It is intentionally distinct from
`GenerationProvider`, whose `generate` method means one model interaction.

```baml
class OpenAi {
  implements Provider {}

  implements DriveProvider {
    function drive<T>(self, task: Task<T>) -> Response<T> {
      self.generate<T>(task.with_provider(self))
    }
  }
}

class Agent {
  inner: ToolCallingProvider,
  policy: AgentOptions,

  implements Provider {}

  implements DriveProvider {
    function drive<T>(self, task: Task<T>) -> Response<T> {
      // Runs the complete tool loop and must finish as T.
      drive_agent_to_completion(self.inner, task, self.policy)
    }
  }
}
```

Therefore changing `$provider` may intentionally change the direct call's
execution behavior, not merely its model name. The contract remains stable:
the call returns the declared `T`, or throws. Budget stops, handoffs, and other
non-value outcomes are available through explicit `run_agent`; a provider's
default `drive` must resolve them according to its documented policy rather
than widening the LLM function's return type.

Because a `DriveProvider` may execute tools or other effects, its default
replay policy is `Never`. A one-generation provider may truthfully report
`Safe`; an `Agent` normally retries only inside its own loop before a side
effect commits.

All compiler-injected LLM-function controls use `$...` names. Both
`MyFunction(..., $provider = P)` and
`MyFunction.task(..., $provider = P)` select `P`; the first drives immediately
and the second only returns the task value. V1 specifies `$provider`. Budgets,
hooks, and dynamic tools remain explicit driver options.

Page 1 compares this design with Vercel AI SDK, Pydantic AI, LangChain, and
DSPy. The ingredients have precedents; the uncommon part is letting one
compiler-declared function be called directly, turned into `Task<T, P>`, and
provider-overridden without separating the task contract into an agent object.

## Why drivers, not lifecycle companions

Generating lifecycle companions such as `.stream`, `.with_meta`,
`.background`, `.agent`, `.prompt`, and `.parse` would make the compiler own
execution policy. The compiler instead owns only task construction:

```text
compiler: declaration -> typed Task<T>
stdlib:   Task<T> -> execution policy
provider: semantic capability -> wire protocol
```

```baml
let task = ExtractInvoice.task(scan)
let result = ai.drivers.run_agent(task)
```

A third-party driver is an exported generic function, immediately usable with
every existing task. No compiler registration or generated SDK member is
needed.

## Initial driver surface

The initial catalog is deliberately broad enough that normal users should not
need provider HTTP APIs:

| Family | Drivers |
| --- | --- |
| provider default | `drive`, `drive_with_meta` |
| single generation | `generate`, `generate_with_meta`, `stream` |
| agent/tool loop | `run_agent`, `stream_agent` |
| deferred | `submit_background`, `submit_batch` |
| conversation | `open_session`, `run_in_session` |
| realtime | `open_live` |
| managed context | `create_cache` |
| media | `generate_image`, `transcribe`, `generate_speech` |
| vector/ranking | `embed`, `rerank` |
| external orchestration | `submit_harness` |

Inspection and offline parse live outside the execution namespace:
`ai.inspect.prompt(task)`, `ai.inspect.messages(task)`, `ai.parse(task, text)`.

Streaming accepts a compiler-known
`StreamTask<T, baml.macros.stream_type!(T), P>` projection. This is type/PPIR
projection, not another generated function.

## Safe and runtime-negotiated drivers

The preferred safe surface requires capability evidence:

```baml
drivers.drive<T, P extends DriveProvider>(task: Task<T, P>) -> T
drivers.run_agent<T, P extends ToolCallingProvider>(task: Task<T, P>) -> AgentRun<T>
```

The provider type is inferred from the declaration default or `$provider`
override. If it is known, incompatibility is a compile error. Routing code may
explicitly erase it to `Task<T>` and use `drivers.unsafe.*`.

Dynamic routing erases that evidence, so the explicit fallback is:

```baml
ai.drivers.unsafe.drive(task)
ai.drivers.unsafe.run_agent(task)
```

These functions match the provider interface at runtime and then invoke the
safe driver. They still validate schemas, respect replay rules, and return a
typed `Unsupported` when capability evidence is absent.

## Providers and capability escape hatches

`Provider` is the pure binding/rendering contract: it exposes a display
descriptor and provider-sensitive prompt context, but performs no I/O.
Concrete classes opt into semantic execution-capability interfaces:

```baml
class OpenAi {
  model: string,
  api_key: string,

  implements Provider {}
  implements DriveProvider { ... }       // behavior of direct MyFunction(...)
  implements GenerationProvider { ... }    // exactly one model interaction
  implements StreamingProvider { ... }
  implements ToolCallingProvider { ... }
}
```

Provider capability interfaces use the `*Provider` suffix. Data and resource
interfaces (`Messages`, `Transcript`, `Session`), policies (`RetryPolicy`),
hooks (`AgentHooks`), and syntax-only extensions (`ProviderSugar`) do not.
Concrete provider values keep concise names such as `OpenAi`, `Anthropic`,
`Agent`, `Retry`, and `Fallback`.

`ToolCallingProvider` is intentionally not named `AgentProvider`: it supplies
one provider step and does not own the loop. `Agent` is a concrete provider
composition that packages a `ToolCallingProvider` with loop policy and exposes
that behavior through `DriveProvider`.

Provider authors implement semantic methods such as `generate`, `step`, or
`open_live`; request JSON and response codecs stay private. A custom driver may
call a known capability directly:

```baml
function custom_run<T>(p: GenerationProvider, task: Task<T>) -> T {
  p.generate<T>(task.with_provider(p)).value
}
```

This is the supported low-level escape hatch. Raw `baml.http.fetch` remains
available to provider implementors, but should not be attractive to application
authors.

## Tools and agent mutation

Tools may be declared on the LLM function, configured as provider-owned tools,
supplied in driver options, or changed by `prepare_step`. The driver recomputes
the active roster before every model turn.

```baml
class MyHooks {
  implements AgentHooks {
    function prepare_step(self, ctx: StepContext) -> StepPlan {
      if (ctx.step == 2) {
        let mcp = baml.mcp.connect(discover_server())
        ctx.tool_registry.add_all(mcp.tools())
      }
      StepPlan {
        provider: if (ctx.usage.cost_usd > 0.25) { CheapModel } else { null },
        tools: ctx.tool_registry.snapshot(),
        stop: null,
      }
    }
  }
}

let result = ai.drivers.run_agent(
  Research.task(question),
  AgentOptions { hooks: MyHooks {} },
)
```

A tool can authorize/discover more tools by updating a driver-owned registry.
Name collisions are errors unless replacement is explicit.

## Messages and transcripts

`MessagePart`, `Message`, `Messages`, and `Transcript` are interfaces.
`Conversation` is the standard editable/serializable neutral implementation.

The observability representation must not automatically become the mutable
source of truth for future provider calls:

```text
application owns: tool dispatch, logging, UI, business state, Conversation
provider owns:    exact wire history, signatures, opaque blocks, continuation
driver owns:      active Transcript during the run
```

Exact persistence uses provider-controlled sealed tokens:

```baml
interface ResumableToolCallingProvider requires ToolCallingProvider {
  function save_transcript(self, transcript: Transcript) -> TranscriptToken
  function restore_transcript(self, token: TranscriptToken) -> Transcript
}
```

Cross-provider switching is explicitly export/import:

```baml
interface TranscriptImportProvider requires ToolCallingProvider {
  function import_conversation(self, conversation: Conversation) -> TranscriptImport
}
```

`TranscriptImport` reports `Exact`, `MessagesOnly`, or `Lossy`, plus warnings.
Provider-private reasoning signatures, encrypted blocks, caches, and server
handles may not transfer. Drivers emit the fidelity when a hook changes the
provider halfway through an agent loop.

## Resources

Operations whose follow-ups depend on provider/harness-owned state return
resources: `Job<T>`, `Batch<T>`, `Session`, `Live`, `Cache`, `HarnessRun<T>`.
Resources own their lifecycle and provider binding. Serializable opaque tokens
cross process boundaries; resources themselves do not.

```baml
let job = ai.drivers.submit_background(
  DeepResearch.task(topic),
  BackgroundOptions { idempotency_key: "research-42" },
)
defer { job.cleanup() }
```

Application-owned tool loops cannot run inside a provider background job.
They use `submit_harness` or a durable workflow so an application worker owns
dispatch and replay.

## Reliability and fluent sugar

Errors report retry/effect/refusal predicates; operation replay policy decides
whether retry is safe. Retry, fallback, and tracing are provider wrappers.

Dot syntax is supplied through out-of-body blanket implementations, not by
polluting `Provider`:

```baml
interface ProviderSugar requires Provider {
  function with_retry(self, policy: RetryPolicy) -> Retry { ... }
  function fallback_to(self, other: Provider) -> Fallback { ... }
  function traced(self, meter: UsageMeter) -> Traced { ... }
}

implements<T extends Provider> ProviderSugar for T {}
```

Libraries may add opinionated sugar such as `.judged_by(...)` through their
own interface and out-of-body blanket implementation. Sugar delegates to an
ordinary wrapper constructor and is never capability evidence.

## Extension model

To add a cross-provider interaction shape:

1. declare `interface XProvider requires Provider`;
2. implement it on provider classes, including out-of-body implementations;
3. write a safe driver constrained to `X`;
4. optionally write `drivers.unsafe.x` for erased providers;
5. consume any LLM function's `.task(...)`.

To extend a built-in provider without editing its declaration, write an
out-of-body `implements X for OpenAi { ... }` in your package. To add syntax
sugar, define a separate fluent interface and blanket implementation. Neither
extension changes compiler codegen.

## Workflows

Durable workflows compose tasks and drivers but are not provider capabilities.
The workflow executor owns journals, timers, signals, workers, replay, and
idempotency. An LLM step is simply:

```baml
ctx.step("extract", () -> Invoice {
  ExtractInvoice(input)
})
```

Provider job tokens and transcript tokens may be stored as workflow state;
provider values and process-local tasks may not.

## Resolved decisions

1. `.task(...)` is the sole generated LLM-function companion and is resolved
   only on an LLM-function declaration path; ordinary function values do not
   acquire a `task` member.
2. Plain calls lower to `drivers.drive(task)`, which invokes the selected
   provider's `DriveProvider` implementation.
3. `$provider` is a reserved synthetic parameter and a field of `Task<T>`.
4. Lifecycle behavior lives in standard/custom drivers.
5. Streaming partial projection is compiler-known type information, not a
   generated lifecycle function.
6. Safe drivers prefer static capability constraints; `unsafe` means runtime
   negotiation only.
7. Tool rosters may change between steps; `null` inherits/keeps tools while
   `[]` explicitly clears application tools.
8. Provider switching performs explicit transcript export/import and reports
   fidelity.
9. Neutral conversations are app-owned; exact transcripts are provider-owned.
10. Direct semantic provider capabilities are the supported low-level escape
    hatch; raw vendor HTTP is provider-implementation territory.
11. After BEP-062, ordinary function values are the canonical executable
    application-tool primitive; the erased `Tool` boundary still supports MCP
    and other runtime-schema tools.

## Open questions

1. Final type-system spelling that preserves the provider capability on
   `Task<T>` while allowing existential tasks.
2. Whether `$provider` is exposed with the sigil in every generated host SDK or
   mapped to an idiomatic reserved override name.
3. Whether `StreamTask` is a visible wrapper type or only a type-checker
   projection at a `drivers.stream` call.
4. Which specialized media/vector drivers are v1 versus staged immediately
   after `run`/`stream`/agent/resources.
