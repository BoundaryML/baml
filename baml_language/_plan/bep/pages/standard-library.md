# Standard Library Surface

This page specifies the intended public organization of `baml.ai`. It separates semantic provider capabilities from implementation helpers and explains which built-in provider classes should implement which capabilities.

The table is a target contract; not every cell is required in the first implementation phase.

## Package map

```text
baml.ai
├── core
│   ├── Provider
│   ├── LlmRequest<T>
│   ├── LlmResponse<T>
│   ├── ResponseMeta
│   ├── ChatMessage / MessagePart
│   └── request<T>, run<T>, run_with_meta<T>
├── capabilities
│   ├── Generate
│   ├── Streaming
│   ├── Tools
│   ├── Background / Job<T>
│   ├── Batching / Batch<T>
│   ├── Sessions / Session
│   ├── Realtime / LiveSession
│   ├── ManagedCache / CacheResource
│   ├── Inspectable
│   └── PromptInfo
├── composition
│   ├── retry
│   ├── fallback
│   ├── round_robin
│   ├── traced
│   ├── cached
│   └── tool_loop
├── providers
│   ├── OpenAi
│   ├── OpenAiResponses
│   ├── OpenAiRealtime
│   ├── Anthropic
│   ├── Gemini
│   ├── GeminiLive
│   └── OpenAiCompatible
└── internal
    ├── HTTP/auth helpers
    ├── SSE/WebSocket decoders
    ├── provider wire classes
    └── SAP/JSON-schema adapters
```

Directories are organizational. Public names remain under `baml.ai` unless the language adopts nested import conventions.

## Core request and response

### `Provider`

```baml
interface Provider {}
```

It is the existential root for dynamic provider selection. It has no retry, fallback, transport, or lifecycle methods.

### `PromptInfo`

```baml
interface PromptInfo requires Provider {
  function prompt_info(self) -> PromptProviderInfo throws never
}
```

`PromptInfo` supplies model/provider metadata used while rendering `ctx.client`-style prompt expressions. A provider without it gets a stable neutral descriptor. Prompt rendering must never require a network call.

### `LlmRequest<T>`

```baml
class LlmRequest<T> {
  provider: Provider,
  prompt: baml.llm.PromptAst,
  identity: LlmFunctionIdentity?,
  arguments: map<string, unknown>,
  options: RequestOptions,
  tags: map<string, string>,

  function messages(self) -> ChatMessage[] throws never
  function output_type(self) -> type throws never
  function for_provider(self, provider: Provider) -> LlmRequest<T> throws never
  function with_options(self, options: RequestOptions) -> LlmRequest<T> throws never
  function with_tags(self, tags: map<string, string>) -> LlmRequest<T> throws never
}
```

Mutation-style helpers return a new request. They must not evaluate the model. `for_provider` re-evaluates the retained prompt render recipe with provider-sensitive context; it does not perform provider I/O.

`ChatMessage` and `MessagePart` remain the provider-neutral content view. `ChatMessage.text() -> string?` returns concatenated text only when every part is text; it returns `null` when flattening would lose media. Text-only providers use this helper to reject unsupported payloads explicitly.

### `RequestOptions`

Only portable semantics belong here:

```baml
class RequestOptions {
  temperature: float?,
  max_output_tokens: int?,
  stop: string[]?,
  seed: int?,
  provider: map<string, unknown>,
}
```

`provider` is an explicit escape hatch. Standard provider classes SHOULD also expose typed construction helpers for their own options so users do not have to guess string keys.

### `LlmResponse<T>` and `ResponseMeta`

```baml
class LlmResponse<T> {
  value: T,
  meta: ResponseMeta,
}

class ResponseMeta {
  provider: string,
  model: string?,
  request_id: string?,
  finish_reason: string?,
  usage: Usage?,
  attributes: map<string, unknown>,
  raw: json?,
}
```

Metadata is a value, not a callback projection executed inside a retry scope.

## Capabilities

### `Generate`

```baml
interface Generate requires Provider {
  function generate<T>(self, request: LlmRequest<T>) -> LlmResponse<T>
    throws baml.errors.GenerateError | baml.errors.UnknownError
}
```

This is the normal request/response operation. It is not HTTP-specific.

### `Streaming`

```baml
interface Streaming requires Provider {
  function stream<TPartial, T>(
    self,
    request: LlmRequest<T>,
  ) -> baml.llm.Stream<TPartial, T>
    throws baml.errors.StreamError | baml.errors.UnknownError
}
```

The stream owns partial and final parsing plus late metadata. The stream API SHOULD expose final metadata without changing the final `T`, for example `stream.response()` after completion.

### `Tools`

```baml
interface Tools requires Provider {
  type Transcript

  function begin<T>(self, request: LlmRequest<T>, tools: Tool[]) -> Self.Transcript
  function step<T>(self, transcript: Self.Transcript) -> T | ToolCalls
  function submit(self, transcript: Self.Transcript, results: ToolResult[]) -> Self.Transcript
}
```

The stdlib `run_tools` driver owns the loop, budget, dispatch, handoff, and parallelism policy. A provider may alternatively expose managed server tools through normal `Generate` configuration when no client-side loop exists.

### `Background`

```baml
interface Background requires Provider {
  function submit<T>(self, request: LlmRequest<T>, options: BackgroundOptions) -> Job<T>
  function resume<T>(self, token: JobToken) -> Job<T>
}
```

See [Background jobs](./background-jobs.md).

### `Batching`

```baml
interface Batching requires Provider {
  function submit_batch<T>(
    self,
    requests: KeyedRequest<T>[],
    options: BatchOptions,
  ) -> Batch<T>

  function resume_batch<T>(self, token: BatchToken) -> Batch<T>
}
```

Batch item identity is caller-provided because results may be unordered. A batch resource exposes counts, cancellation, and a result iterator.

### `Sessions`

```baml
interface Sessions requires Provider {
  function open_session(self, options: SessionOptions) -> Session
  function resume_session(self, token: SessionToken) -> Session
}
```

`Session.run<T>(request)` executes an LLM request within the provider-owned conversation. Optional narrower interfaces such as `ForkableSession` and `CompactableSession` express extra resource behavior.

### `Realtime`

```baml
interface Realtime requires Provider {
  function open_live<T>(self, request: LlmRequest<T>, channel: Channel) -> LiveSession
}
```

`LiveSession` owns the connection and exposes events, send operations, response cancellation, audio truncation when supported, transcript/final state, and cleanup.

### `ManagedCache`

```baml
interface ManagedCache requires Provider {
  function create_cache(self, prefix: ChatMessage[], options: CacheOptions) -> CacheResource
  function resume_cache(self, token: CacheToken) -> CacheResource
}

interface CacheResource {
  function run<T>(self, request: LlmRequest<T>) -> T
  function run_with_meta<T>(self, request: LlmRequest<T>) -> LlmResponse<T>
  function token(self) -> CacheToken
  function delete(self) -> void
  function cleanup(self) -> void
}
```

This capability exists only for explicit provider-managed cache resources. Implicit prompt caching is observed in metadata and may be influenced by provider options, but it does not return a resource.

### `Inspectable`

```baml
interface Inspectable requires Provider {
  function build_request<T>(self, request: LlmRequest<T>) -> RequestPreview
    throws baml.errors.InspectError
}
```

`RequestPreview` may contain an HTTP request, a local-runtime invocation, or another transport-neutral representation. Inspection performs no provider I/O and redacts credentials by default.

## Implementation helpers are not capabilities

The stdlib may offer reusable helpers such as:

```baml
interface HttpCodec {
  type WireResponse
  function build<T>(self, request: LlmRequest<T>) -> baml.http.Request
  function decode<T>(self, response: Self.WireResponse) -> LlmResponse<T>
}
```

This is a library protocol for reducing provider implementation duplication. A retry wrapper, local model, or realtime provider need not implement it. Runtime capability negotiation never asks whether a provider is an `HttpCodec`.

The same distinction applies to:

- OAuth/SigV4 helpers;
- SSE accumulators;
- WebSocket event codecs;
- multipart upload helpers;
- JSON navigation;
- SAP parsing;
- JSON-schema lowering.

## Intended built-in provider coverage

Legend: **yes** is part of the provider contract; **separate class** means a distinct provider class should own the capability; **API-dependent** requires a concrete endpoint/model and may be exposed through a descriptor.

| Provider              | Generate |     Streaming |            Tools |    Background |      Batching |      Sessions | Realtime |                                        Managed cache |
| --------------------- | -------: | ------------: | ---------------: | ------------: | ------------: | ------------: | -------: | ---------------------------------------------------: |
| `OpenAi` (chat-style) |      yes |           yes |              yes |            no |  separate/API |            no |       no |                                        implicit only |
| `OpenAiResponses`     |      yes |           yes |              yes |           yes |  separate/API |           yes |       no |                                        implicit only |
| `OpenAiRealtime`      |       no |            no | via live session |            no |            no |  live session |      yes |                                                   no |
| `Anthropic`           |      yes |           yes |              yes |            no |           yes | API-dependent |       no | explicit prompt controls, not necessarily a resource |
| `Gemini`              |      yes |           yes |              yes | API-dependent | API-dependent | API-dependent |       no |                API-dependent explicit/implicit modes |
| `GeminiLive`          |       no |            no | via live session |            no |            no |  live session |      yes |                                                   no |
| `OpenAiCompatible`    |      yes | API-dependent |    API-dependent |            no |            no |            no |       no |                                                   no |

The provider classes SHOULD be split when one endpoint has a materially different wire protocol or lifecycle. For example, a realtime WebSocket provider should not implement a degenerate `Generate` merely so it looks like a chat provider.

## Provider descriptors

Interfaces answer “does this operation exist?” Descriptors answer payload/model questions:

```baml
enum Support {
  Yes,
  No,
  Maybe,
}

interface DescribedProvider requires Provider {
  function support(self, feature: Feature, request: RequestShape?) -> Support
  function limits(self) -> ProviderLimits
}
```

Examples:

- `Generate` exists, but video input is `No` for this model.
- `Streaming` exists, but structured partial output is `Maybe` for this schema.
- `Tools` exists, but parallel calls are `No`.
- the context limit is 128,000 tokens.

Descriptors improve planning and diagnostics. The actual operation remains the source of truth because remote support can change and payload validation is authoritative.

## Composition helpers

Composition is exposed as functions rather than inherited methods on `Provider`:

```baml
let robust = baml.ai.retry(
  baml.ai.fallback([primary, secondary]),
  policy,
)
```

This keeps the provider marker minimal and makes imports/autocomplete explicit.

### `retry`

Returns a wrapper that implements only the capabilities for which it has a valid replay strategy. `Generate` is the initial required implementation. Background submission requires an idempotency key. Realtime is not automatically retried.

### `fallback`

Selects another provider only before the operation has committed or produced observable output. Member capability coverage is an intersection, not a union: the wrapper can promise `Streaming` only if its fallback semantics and members support it.

### `round_robin`

Chooses a member before request rendering when prompt context depends on the provider. The request is then rendered for and bound to the chosen provider.

### `traced`

Records each attempt and resource transition. It wraps returned jobs/sessions/live connections so later operations remain traced.

### `cached`

Framework response caching is a `Generate` wrapper keyed by semantic request identity. It is distinct from provider-managed prompt caches.

### `tool_loop`

Adapts a `Tools` provider into `Generate` for a configured tool roster and dispatch policy. It is the right wrapper when callers want plain LLM-function invocation to run the whole loop automatically.

```baml
let AgentModel = baml.ai.tool_loop(ToolModel, tools, dispatch, budget)
let answer = ResearchQuestion(question, client = AgentModel)
```

## Error packages

Each operation family has a stable error interface:

```text
baml.errors.GenerateError
baml.errors.StreamError
baml.errors.ToolError
baml.errors.BackgroundError
baml.errors.BatchError
baml.errors.SessionError
baml.errors.RealtimeError
baml.errors.CacheError
baml.errors.InspectError
```

Concrete provider errors implement the relevant interfaces. `baml.errors.Unsupported` can appear on any driver boundary and includes the requested capability plus provider identity.

See [Effects, errors, and testing](./effects-errors-testing.md).
