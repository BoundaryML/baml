# 3. Drivers

Drivers decide how a `Task<T>` executes. They are ordinary standard-library
functions, not compiler-generated task members. This keeps code generation
bounded to `MyFunction.task(...)`, makes lifecycle policy explicit, and lets
applications and libraries write new drivers without compiler hooks.

Drivers live under `ai.drivers`. `ai` is a top-level namespace, parallel to
`baml` and `assert`; it is not nested under `baml`.

## Default catalog

The initial standard library should ship the following families.

| Driver | Input | Result | Purpose |
| --- | --- | --- | --- |
| `drive` | `Task<T>` | `T` | selected provider's default complete behavior |
| `drive_with_meta` | `Task<T>` | `Response<T>` | default complete behavior plus metadata |
| `generate` | `Task<T>` | `T` | force exactly one model-generation interaction |
| `generate_with_meta` | `Task<T>` | `Response<T>` | one interaction plus metadata |
| `stream` | `StreamTask<T, TPartial>` | `Stream<TPartial, T>` | incremental structured generation |
| `run_agent` | `Task<T>` | `AgentRun<T>` | full tool loop with explicit terminal outcome |
| `stream_agent` | `Task<T>` or `StreamTask<T, TPartial, P>` | `AgentEventStream<T>` | tool loop as observable events; stream tasks also emit typed partial-output events |
| `submit_background` | `Task<T>` + options | `Job<T>` | provider-owned deferred work |
| `submit_batch` | `Task<T>[]` + provider/options | `Batch<T>` | provider batch lifecycle |
| `open_session` | provider + options | `Session` | provider-owned conversation resource |
| `run_in_session` | session + `Task<T>` | `Response<T>` | execute in an existing session |
| `open_live` | `Task<T>` + channel/options | `Live` | realtime duplex resource |
| `create_cache` | provider + messages/options | `Cache` | provider-managed context cache |
| `generate_image` | image task/options | `ImageResult` | image generation/editing |
| `transcribe` | transcription task/options | `TranscriptionResult` | speech-to-text |
| `generate_speech` | speech task/options | `SpeechResult` | text-to-speech |
| `embed` | embedding task/options | `EmbeddingResult` | embeddings |
| `rerank` | rerank task/options | `RerankResult` | ranking |
| `submit_harness` | harness + `Task<T>` | `HarnessRun<T>` | externally managed/durable agent harness |

The non-language-model media operations may use specialized task types where
their inputs and outputs are not prompt-shaped. They still follow the same
rule: a task is intent; a driver owns execution and lifecycle.

Drivers may accept both task flavors when partial typing adds value. For
example, `stream_agent(Task<T, P>)` emits lifecycle/tool/model events, while
`stream_agent(StreamTask<T, TPartial, P>)` additionally emits typed
`PartialOutput<TPartial>` events. A driver that cannot use partials accepts
only `Task<T, P>`.

Inspection is not execution:

```baml
ai.inspect.prompt(task)     // PromptAst
ai.inspect.messages(task)   // Messages
ai.parse<T>(task, text)     // T, no network call
```

## Common examples

```baml
let task = ExtractInvoice.task(scan)

let invoice = ai.drivers.drive(task)
let response = ai.drivers.drive_with_meta(task)
let one_turn = ai.drivers.generate_with_meta(task)
let stream = ai.drivers.stream(task)
let outcome = ai.drivers.run_agent(task)
let events = ai.drivers.stream_agent(task)
let job = ai.drivers.submit_background(task, options)
```

The plain source call remains equivalent to the first line:

```baml
ExtractInvoice(scan)
// exactly: ai.drivers.drive(ExtractInvoice.task(scan))
```

## Safe drivers and unsafe drivers

Safe drivers state their capability requirements in their input types. For
concrete providers, unsupported combinations should fail statically:

```baml
function drive<T, P extends ai.DriveProvider>(task: Task<T, P>) -> T
function generate<T, P extends ai.GenerationProvider>(task: Task<T, P>) -> T
function run_agent<T, P extends ai.ToolCallingProvider>(task: Task<T, P>) -> AgentRun<T>
```

`drive` is deliberately thin; the provider owns the default policy:

```baml
function drive<T, P extends ai.DriveProvider>(task: Task<T, P>) -> T {
  task.$provider.drive<T>(task).value
}
```

This means swapping `$provider` can swap the default execution strategy. It
does not change the LLM function's declared return type.

`Task<T, P>` carries the concrete provider type selected by the declaration or
`$provider` override. `Task<T>` means `P = Provider` after intentional erasure.
The default API makes the caller prove the required capability whenever that
evidence has not been erased.

Dynamic routing sometimes erases the provider to `Provider`. For that case,
every standard driver has an explicitly unsafe negotiation spelling:

```baml
ai.drivers.unsafe.drive(task)
ai.drivers.unsafe.run_agent(task)
ai.drivers.unsafe.stream(task)
```

An unsafe driver performs a runtime interface match, then calls the same safe
driver. Absence returns/throws typed `Unsupported`; “unsafe” never means
unchecked memory or unvalidated provider output. It means only that capability
compatibility is confirmed at runtime instead of compile time.

```baml
function unsafe.run_agent<T>(task: Task<T>) -> AgentRun<T> {
  match (task.$provider) {
    let p: ToolCallingProvider => drivers.run_agent(task.with_provider(p)),
    _ => throw Unsupported { capability: "tool calling", provider: task.provider_name() },
  }
}
```

## Hooks and per-step decisions

Agent drivers accept a policy object whose hooks may observe or alter the next
step without owning the transcript:

```baml
interface AgentHooks {
  function prepare_step(self, context: StepContext) -> StepPlan throws never
  function before_tool_call(self, event: BeforeToolCall) -> ToolDecision throws never
  function after_tool_call(self, event: AfterToolCall) -> void throws never
  function on_event(self, event: AgentEvent) -> void throws never
}

class StepPlan {
  provider: Provider?,   // switch provider for the next model turn
  tools: Tool[]?,        // replace the active roster for the next turn
  stop: AgentStop?,
}
```

`prepare_step` is the standard place to change providers, activate a newly
connected MCP server, prune tools, change budgets, or force final synthesis.
Observers receive immutable event views. They are not automatically the
mutable source of truth for the next provider request.

## Dynamic tools

Application tools resolve in this order:

```text
base = options.tools ?? task.tools
if a ToolRegistry is attached: base = registry.snapshot()
if prepare_step(...).tools is non-null: base = that replacement
```

`null` means “inherit/keep the current roster”; `[]` means “intentionally
expose no application tools.” An empty array is never an inheritance sentinel.
Once attached, a live `ToolRegistry` is authoritative because it can add and
remove tools between turns. Provider-owned tools remain typed provider
configuration and are translated separately by the provider adapter.

Names must be unique in the resulting request. A later source does not
silently shadow an earlier one; collisions are typed errors unless an explicit
replacement API is used.

An MCP connection is a runtime tool source. It can be added halfway through
the loop:

```baml
function prepare_step(ctx: StepContext) -> StepPlan {
  if (ctx.step == 3 && ctx.state.get("mcp") == null) {
    let mcp = baml.mcp.connect(discover_server(ctx))
    ctx.tool_registry.add_all(mcp.tools())
  }
  StepPlan { provider: null, tools: ctx.tool_registry.snapshot(), stop: null }
}
```

A tool may itself discover or authorize more tools, but it mutates a
driver-owned `ToolRegistry`; it does not rewrite the task declaration.

## Provider switching halfway through a loop

Changing `StepPlan.provider` is a semantic handoff, not a field swap:

1. The driver asks the old transcript for its provider-neutral `Conversation`
   view.
2. The target provider imports that view into a new native `Transcript`.
3. The task prompt is re-rendered using `task.with_provider(target)`.
4. The loop continues with the target transcript and current tool roster.

A non-null `StepPlan.provider` always requests this handoff. The driver MUST
NOT skip it because `descriptor()`, family, model, or label happens to equal
the current provider's display data. Separately configured provider values can
share every displayed field while differing in credentials, endpoints,
middleware, or private continuation state.

The conversion reports its fidelity. Provider-private reasoning signatures,
encrypted blocks, caches, or server-side state may not transfer. Exact
continuation is available only through the owning provider's sealed transcript
token. Page 5 defines these interfaces and rules.

## Custom drivers

A custom driver is an ordinary generic function:

```baml
function run_moderated<T>(task: Task<T>, policy: string) -> ModeratedResponse<T> {
  match (task.$provider) {
    let p: ModeratedGenerationProvider => p.generate_moderated<T>(task, policy),
    _ => throw Unsupported { capability: "moderated generation", provider: task.provider_name() },
  }
}

let result = run_moderated(ComposeNote.task(topic), "no-pii")
```

No task member, registry entry, compiler plugin, or SDK-specific companion is
required. Host SDKs expose `.task(...)` plus the standard driver namespace;
third-party drivers are generated like ordinary exported functions.

## Why no lifecycle modifiers

Generating `.stream`, `.with_meta`, `.background`, `.agent`, `.prompt`, and
`.parse` on every LLM function would make the compiler choose the platform's
lifecycle vocabulary. A single `.task` companion keeps the ownership boundary
strict:

```text
compiler owns: declaration -> typed intent
library owns:  intent -> execution policy
provider owns: policy operation -> wire protocol
```

A new driver is immediately usable with every existing LLM function.
