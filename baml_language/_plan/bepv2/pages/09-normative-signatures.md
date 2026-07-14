# 9. Normative Signatures

This page is the compact contract. Prose elsewhere yields to it. Names are
provisional; ownership and behavior are normative.

Labels: **[lang]** ordinary BAML, **[syntax]** compiler surface,
**[runtime]** runtime representation, **[stdlib]** library API, **[sdk]** host
SDK generation.

## Namespace — [stdlib]

`ai` is a top-level namespace, parallel to `baml` and `assert`. Provider types,
tasks, drivers, resources, tools, transcripts, and related policies are named
`ai.*`; the namespace is never nested under `baml`.

## LLM function surface

### Declaration fields — [syntax]

```baml
function Name(args...) -> T {
  provider: <DriveProvider expression>       // required because Name(...) is callable
  tools: <Tool[] expression>          // optional
  prompt: <prompt template>
}
```

`client:` is a compatibility spelling for `provider:`.

### Sole generated companion — [syntax] [sdk]

`.task(...)` is a compiler-resolved selector on an LLM-function declaration
path, not a runtime member of an ordinary callable value:

```baml
Name.task(args...) // valid when Name resolves directly to an LLM declaration

let callable = Name
callable(args...)      // valid direct call
callable.task(args...) // compile error: callable values have no `task` member
```

The selector may lower to a hidden generated companion item, but V1 guarantees
only the call-position form `Name.task(...)`. Name resolution MUST NOT recover
the selector using local-value provenance or constant propagation. A future
first-class LLM-function value type may expose task construction explicitly;
ordinary function types do not.

Let `P_default` denote the static type inferred for the declared `provider:`
expression. Let `P` denote the static type inferred for an explicit override.
Neither name is emitted into BAML source.

```text
Name.task(args...)                       -> Task<T, P_default>
Name.task(args..., $provider = provider) -> Task<T, P>
```

The LLM function remains directly callable with the injected parameter:

```text
Name(args...)                       -> T    requires P_default: DriveProvider
Name(args..., $provider = provider) -> T    requires P: DriveProvider
```

Direct calls require `DriveProvider`; `.task` accepts any `Provider` for use by an
explicit driver.

There is no `typeof(value)` operator in this contract. Omission of `$provider`
is special LLM-function call typing: the compiler injects the declared provider
expression and retains its already-inferred static type. `reflect.type_of<T>()`
instead returns a runtime `type` value for an already-known type argument and
is used by `Task.output_type()`, JSON Schema generation, and parsing.

No lifecycle-specific companion is generated. A direct `Name(args...)` call
lowers to `ai.drivers.drive(Name.task(args...))`.

The compiler also knows the type projection
`StreamTask<T, baml.macros.stream_type!(T), P>` when a task flows to the stream
driver; this is not another generated function.

## Task — [runtime] [stdlib]

```baml
class Task<T, P extends Provider = Provider> {
  $provider: P,
  prompt: baml.llm.PromptAst,
  identity: TaskIdentity?,
  arguments: map<string, unknown>,
  tools: Tool[],
  options: TaskOptions,
  tags: map<string, string>,
  transcript: Transcript?,
  _render: PromptRenderRecipe,

  function messages(self) -> Messages throws never
  function output_type(self) -> type throws never
  function provider_name(self) -> string throws never
  function with_provider<Q extends Provider>(self, provider: Q) -> Task<T, Q> throws never
  function with_tools(self, tools: Tool[]) -> Task<T, P> throws never
  function with_transcript(self, transcript: Transcript) -> Task<T, P> throws never
}

class PromptRenderRecipe {
  template: (baml.llm.Context) -> baml.llm.PromptAst,
  output_type: type,

  function render<P extends Provider>(self, provider: P) -> baml.llm.PromptAst throws never {
    self.template(provider.prompt_context(self.output_type))
  }
}

class TaskIdentity { name: string, package: string }
class TaskOptions {
  tool_registry: ToolRegistry?,
  tool_middleware: ToolMiddleware[],
  observers: AgentObserver[],
  recorders: AgentRecorder[],
  run_id: string?,
}

function ai.task<T, P extends Provider>(
  provider: P,
  template: (baml.llm.Context) -> baml.llm.PromptAst,
) -> Task<T, P>
```

`Task<T, P>` is process-local. `P` retains static capability evidence;
`Task<T>` is shorthand for the existentially erased `Task<T, Provider>`.
`with_provider<Q>` re-renders from `_render` and changes the provider type to
`Q`.

## Provider and generation capabilities — [lang] [stdlib]

Provider capability interfaces use the `*Provider` suffix. Resource/data
interfaces, policies, hooks, and syntax-only `*Sugar` interfaces do not.
Concrete provider classes and compositions use concise nouns such as `OpenAi`,
`Agent`, `Retry`, and `Fallback`.

```baml
class ProviderDescriptor {
  family: string,
  model: string?,
  label: string?,
}

interface Provider {
  // Pure display/diagnostic data. Not provider identity or equality.
  function descriptor(self) -> ProviderDescriptor throws never

  // Pure provider-sensitive prompt preparation. Performs no network I/O.
  function prompt_context(self, output_type: type) -> baml.llm.Context throws never
}

interface DriveProvider requires Provider {
  function drive<T>(self, task: Task<T>) -> Response<T>
    throws baml.errors.CallError | baml.errors.UnknownError
  function replay_policy<T>(self, task: Task<T>) -> ReplayPolicy throws never {
    ReplayPolicy { kind: ReplayKind.Never, idempotency_key: null }
  }
}

// One model interaction. This is lower-level than DriveProvider.
interface GenerationProvider requires Provider {
  function generate<T>(self, task: Task<T>) -> Response<T>
    throws baml.errors.CallError | baml.errors.UnknownError
}

interface StreamingProvider requires Provider {
  function stream<TPartial, T>(
    self,
    task: StreamTask<T, TPartial>,
  ) -> baml.llm.Stream<TPartial, T>
    throws baml.errors.StreamError | baml.errors.UnknownError
}

class Response<T> { value: T, meta: Meta, transcript: Transcript? }
class Meta {
  provider: string,
  model: string?,
  request_id: string?,
  finish_reason: string?,
  usage: Usage?,
  attributes: map<string, unknown>,
  raw: json?,
}
class Usage { input_tokens: int, output_tokens: int }
```

## Messages and transcripts — [lang] [stdlib]

```baml
interface MessagePart {
  function kind(self) -> MessagePartKind throws never
  function text(self) -> string? throws never
  function media(self) -> image | audio | video | pdf | null throws never
  function annotations(self) -> map<string, json> throws never
}

interface Message {
  function role(self) -> MessageRole throws never
  function parts(self) -> MessagePart[] throws never
  function provider_metadata(self) -> json throws never
}

interface Messages {
  function items(self) -> Message[] throws never
  function append(self, message: Message) -> Messages throws never
  function to_conversation(self) -> Conversation throws never
}

interface Transcript {
  function provider(self) -> Provider throws never
  function messages(self) -> Messages throws never
  function conversation(self) -> Conversation throws never
}

class TranscriptToken { provider: string, version: int, sealed: string }
enum TranscriptFidelity { Exact, MessagesOnly, Lossy }
class TranscriptImport {
  transcript: Transcript,
  fidelity: TranscriptFidelity,
  warnings: string[],
}
```

`Conversation` is the standard editable, serializable implementation of
`Messages`. A provider transcript is authoritative continuation state;
`conversation()` is a projection, not an exact reconstruction promise.

## Tool capabilities — [lang] [stdlib]

```baml
interface ToolCallingProvider requires Provider {
  function begin<T>(self, task: Task<T>) -> Transcript
    throws baml.errors.ToolError | baml.errors.UnknownError
  function step<T>(self, transcript: Transcript, tools: Tool[]) -> T | ToolCalls
    throws baml.errors.ToolError | baml.errors.UnknownError
  function submit(self, transcript: Transcript, results: ToolResult[]) -> Transcript
    throws baml.errors.ToolError | baml.errors.UnknownError
}

interface ResumableToolCallingProvider requires ToolCallingProvider {
  function save_transcript(self, transcript: Transcript) -> TranscriptToken
  function restore_transcript(self, token: TranscriptToken) -> Transcript
}

interface TranscriptImportProvider requires ToolCallingProvider {
  function import_conversation(self, conversation: Conversation) -> TranscriptImport
    throws baml.errors.TranscriptError
}

class Tool { name: string, description: string, parameters: type }
class ToolCall { id: string, name: string, args: json }
class ToolResult { id: string, output: json, is_error: bool }
class ToolCalls { calls: ToolCall[] }
```

`ToolCallingProvider` does not imply a native vendor tool API. Native adapters
send `Tool[]` through provider request fields. Prompt-backed adapters render
`${ctx.output_format}` for `T | ToolCalls`, append the active tools' concrete
JSON Schemas, and SAP-parse the chosen branch. This rendering happens after
the per-step tool roster is resolved.

## Agent driver contracts — [stdlib]

```baml
class Budget { max_steps: int?, max_cost_usd: float? }
class Done<T> { value: T, meta: Meta, transcript: Transcript }
class BudgetReached { transcript: Transcript, steps_taken: int }
class Handoff { to: string, args: json, transcript: Transcript, steps_taken: int }
type AgentRun<T> = Done<T> | BudgetReached | Handoff

interface AgentHooks {
  function prepare_step(self, context: StepContext) -> StepPlan throws never
  function before_tool_call(self, event: BeforeToolCall) -> ToolDecision throws never
  function after_tool_call(self, event: AfterToolCall) -> void throws never
  function on_event(self, event: AgentEvent) -> void throws never
}

class StepPlan { provider: Provider?, tools: Tool[]?, stop: AgentStop? }
class AgentOptions {
  budget: Budget?,
  // null inherits task tools; [] intentionally clears application tools.
  tools: Tool[]?,
  tool_registry: ToolRegistry?,
  hooks: AgentHooks?,
  observers: AgentObserver[],
  recorders: AgentRecorder[],
}
```

Provider changes require `TranscriptImportProvider` and emit conversion fidelity.
Tool mutations are applied before the next model step and must not silently
shadow names.

`max_steps` counts calls to `ToolCallingProvider.step`, including the final
value-producing call. Every requested tool-call ID receives exactly one result
before the next provider step; blocked calls are submitted as error results.
Tool results correlate by ID, never array position. A non-null
`StepPlan.provider` always requests a switch; descriptors are not identity.

## Resource capabilities — [lang] [runtime] [stdlib]

```baml
interface BackgroundProvider requires Provider {
  function submit<T>(self, task: Task<T>, options: BackgroundOptions) -> Job<T>
  function resume_job<T>(self, token: JobToken) -> Job<T>
}

interface BatchProvider requires Provider {
  function submit_batch<T>(self, tasks: Task<T>[], options: BatchOptions) -> Batch<T>
}

interface SessionProvider requires Provider {
  function open_session(self, options: SessionOptions) -> Session
  function resume_session(self, token: SessionToken) -> Session
}

interface RealtimeProvider requires Provider {
  function open_live<T>(self, task: Task<T>, channel: Channel) -> Live
}

interface ManagedCacheProvider requires Provider {
  function create_cache(self, content: Messages, options: CacheOptions) -> Cache
}
```

`Job<T>`, `Batch<T>`, `Session`, `Live`, and `Cache` are resource interfaces
with provider-owned lifecycle methods, serializable opaque tokens where
resumption exists, and at-most-once `cleanup()`.

## Standard safe drivers — [stdlib]

The provider generic is the capability evidence used by safe drivers.

```baml
function drivers.drive<T, P extends DriveProvider>(task: Task<T, P>) -> T
function drivers.drive_with_meta<T, P extends DriveProvider>(task: Task<T, P>) -> Response<T>
function drivers.generate<T, P extends GenerationProvider>(task: Task<T, P>) -> T
function drivers.generate_with_meta<T, P extends GenerationProvider>(task: Task<T, P>) -> Response<T>
function drivers.stream<TPartial, T, P extends StreamingProvider>(task: StreamTask<T, TPartial, P>)
  -> baml.llm.Stream<TPartial, T>
function drivers.run_agent<T, P extends ToolCallingProvider>(task: Task<T, P>, options: AgentOptions? = null)
  -> AgentRun<T>
function drivers.stream_agent<T, P extends ToolCallingProvider>(task: Task<T, P>, options: AgentOptions? = null)
  -> AgentEventStream<T>
function drivers.stream_agent<TPartial, T, P extends ToolCallingProvider>(
  task: StreamTask<T, TPartial, P>,
  options: AgentOptions? = null,
) -> AgentEventStream<T> // additionally emits PartialOutput<TPartial>
function drivers.submit_background<T, P extends BackgroundProvider>(task: Task<T, P>, options: BackgroundOptions)
  -> Job<T>
function drivers.submit_batch<T, P extends BatchProvider>(provider: P, tasks: Task<T>[], options: BatchOptions)
  -> Batch<T>
function drivers.open_session<P extends SessionProvider>(provider: P, options: SessionOptions) -> Session
function drivers.run_in_session<T>(session: Session, task: Task<T>) -> Response<T>
function drivers.open_live<T, P extends RealtimeProvider>(task: Task<T, P>, channel: Channel) -> Live
function drivers.create_cache<P extends ManagedCacheProvider>(provider: P, content: Messages, options: CacheOptions) -> Cache
```

Initial specialized provider families use the same naming rule:
`ImageGenerationProvider`, `TranscriptionProvider`,
`SpeechGenerationProvider`, `EmbeddingProvider`, and `RerankingProvider`.
Their safe drivers are `generate_image`, `transcribe`, `generate_speech`,
`embed`, and `rerank`. `submit_harness` instead takes a harness capability;
the harness is an execution owner, not a model `Provider`.

## Runtime-negotiated drivers — [stdlib]

```baml
function drivers.unsafe.drive<T>(task: Task<T>) -> T
function drivers.unsafe.drive_with_meta<T>(task: Task<T>) -> Response<T>
function drivers.unsafe.generate<T>(task: Task<T>) -> T
function drivers.unsafe.generate_with_meta<T>(task: Task<T>) -> Response<T>
function drivers.unsafe.stream<TPartial, T>(task: StreamTask<T, TPartial>) -> Stream<TPartial, T>
function drivers.unsafe.run_agent<T>(task: Task<T>, options: AgentOptions? = null) -> AgentRun<T>
// corresponding unsafe spelling for each standard capability driver
```

Each unsafe driver performs one runtime interface match and invokes the safe
driver. Missing capability is typed `Unsupported`. No wire parsing, schema
validation, or effect-safety check is disabled.

## Reliability and composition — [stdlib]

```baml
enum ReplayKind { Safe, RequiresIdempotencyKey, Never }
class ReplayPolicy { kind: ReplayKind, idempotency_key: string? }
function may_replay(policy: ReplayPolicy, failure: baml.errors.Failure) -> bool

function retry(inner: Provider, policy: RetryPolicy) -> Provider
function fallback(members: Provider[]) -> Provider
function traced(inner: Provider, meter: UsageMeter) -> Provider
```

Fluent sugar is provided through out-of-body blanket implementations on
concrete providers. It is syntax convenience, never a capability. Libraries
may define their own wrapper interfaces and blanket implementations.

## Error families — [stdlib]

```baml
interface Failure {
  function kind(self) -> FailureKind throws never
  function commit_state(self) -> CommitState throws never
  function retry_after(self) -> baml.time.Duration? throws never
}

interface CallError requires Failure {}
interface StreamError requires Failure {}
interface ToolError requires Failure {}
interface TranscriptError requires Failure {}
interface SessionError requires Failure {}
interface RealtimeError requires Failure {}
interface BackgroundError requires Failure {}
```

`UnknownError` remains unclassified. Drivers must not infer replay safety from
it.

## Feature requirement summary

| Contract | lang | syntax | runtime | stdlib | sdk |
| --- | :-: | :-: | :-: | :-: | :-: |
| `.task` sole companion and `$provider` | | ✓ | | | ✓ |
| plain-call lowering to provider `DriveProvider` | | ✓ | | ✓ | ✓ |
| `Task<T>` and render recipe | | | ✓ | ✓ | ✓ |
| stream partial projection | | ✓ | ✓ | ✓ | ✓ |
| safe constrained drivers | type-system support | | | ✓ | ✓ |
| `drivers.unsafe.*` negotiation | ✓ | | | ✓ | ✓ |
| message/transcript interfaces | ✓ | | | ✓ | |
| transcript tokens/import fidelity | ✓ | | ✓ | ✓ | |
| hooks, dynamic tools, provider switching | ✓ | | | ✓ | ✓ |
| resources and tokens | ✓ | | ✓ | ✓ | ✓ |
| out-of-body fluent extensions | ✓ | | | ✓ | |
