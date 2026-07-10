> **Status:** DRAFT — written for design review; proposed names do not yet compile on this branch.

# BEP-063: LLM Requests, Providers, and Capabilities

## Summary

This BEP makes the **LLM function** the normal way to describe an AI task and a typed **LLM request** the common value used to execute that task in different ways.

```baml
class Review {
  summary: string,
  risks: string[],
}

function ReviewRepository(repo: string) -> Review {
  client: DefaultModel
  prompt: `
    Review ${repo}.
    ${ctx.output_format}
  `
}

// The ordinary case stays ordinary.
let review = ReviewRepository("boundaryml/baml")

// Streaming is standard enough to keep its familiar companion.
let stream = ReviewRepository$stream("boundaryml/baml")

// A different lifecycle consumes the same generated request.
let job = baml.ai.submit_background(
  ReviewRepository$request("boundaryml/baml", client = LongRunningModel),
  baml.ai.BackgroundOptions { idempotency_key: "review-boundaryml-baml-v1" },
)
```

The proposal has five core ideas:

1. An LLM function defines a reusable, typed task: its arguments, prompt, output type, and default provider.
2. Every LLM function gets a generated `$request` companion that produces `LlmRequest<T>` without executing it.
3. A provider is an ordinary BAML object. It implements only the capability interfaces it actually supports.
4. A capability driver is an ordinary generic function that consumes `LlmRequest<T>` and negotiates against its provider.
5. Stateful operations return resource objects that own provider state and lifecycle methods, rather than returning loose IDs.

There is no global user capability registry. A user adds a capability by declaring an interface, implementing it on a provider, and writing an ordinary driver function. The generated `$request` companion is the stable bridge between every LLM function and every standard or user-defined execution mode.

## Reading guide

This document is the normative design. The companion pages provide tutorial and implementation depth:

- [Getting started](./pages/getting-started.md) teaches the model from an application author's point of view.
- [Writing custom providers and capabilities](./pages/custom-providers.md) is the end-to-end extension guide.
- [Background jobs](./pages/background-jobs.md) follows one stateful operation through authoring, lowering, submission, polling, persistence, and cleanup.
- [Scenario cookbook](./pages/scenario-cookbook.md) gives recipes for common interaction shapes.
- [Standard library surface](./pages/standard-library.md) specifies the built-in capabilities, providers, and wrappers.
- [Compiler and runtime expansion](./pages/under-the-hood.md) shows the generated code and dispatch path.
- [Effects, errors, and testing](./pages/effects-errors-testing.md) defines safe retry/fallback behavior and test layers.
- [Prior art](./pages/prior-art.md) records the external API patterns that shaped this proposal.
- [Changes from the design on this branch](./changes_from_old_design.md) is deliberately separate from the proposal.

## Motivation

Most AI application code begins with a task that is naturally an LLM function:

```baml
function ExtractInvoice(document: pdf) -> Invoice {
  client: AccurateModel
  prompt: `Extract this invoice: ${document}\n${ctx.output_format}`
}
```

The return type is the schema. The prompt is inspectable. The function is callable from BAML and generated host SDKs. Tests and observability can name the task. That should remain the shortest and best-supported path.

The difficulty begins when execution is not a single immediate request:

- return partial values as they stream;
- let the model call local or server-hosted tools;
- continue a provider-stored conversation;
- start work now and poll it later;
- open a realtime audio session;
- create and later delete a provider-managed cache;
- route across providers with retry or fallback;
- use a vendor-specific operation that BAML has never heard of;
- implement a completely new provider in user code.

These operations do not all have the same signature or lifecycle. Treating them as flags on one universal `call` creates invalid combinations and hides important ownership. Generating a new companion on every LLM function for every installed capability creates a different problem: global registration, suffix conflicts, compiler coupling, and an `N functions × M capabilities` API surface.

The missing abstraction is not a larger provider interface. It is a typed value representing **this invocation of this LLM function before it runs**.

## Goals

The design MUST:

- keep plain LLM functions as the default application-facing API;
- preserve typed outputs, typed partial streaming, prompt roles, and media;
- let providers and capabilities be written entirely in user BAML where the required transport primitives exist;
- support dynamic provider routing without making every provider claim every operation;
- support static capability requirements where a function truly needs one operation;
- give custom operations access to the same rendered prompt and output schema as built-in operations;
- represent stateful operations with explicit ownership, polling, cancellation, persistence, and cleanup;
- make retry and fallback safety specific to an operation, not a provider-wide boolean;
- keep compiler-generated API growth bounded;
- make the lowering understandable enough that an SDE1 can debug it from source.

The design SHOULD:

- make wrappers such as moderation, logging, and default options easier than writing a new transport;
- make capability absence a typed error;
- keep provider-specific wire types behind provider implementations;
- allow low-level direct calls when no LLM function is appropriate;
- provide stable inspection points for rendered prompts and requests.

## Non-goals

This BEP does not standardize every vendor option. Vendor-specific settings remain provider configuration or typed provider-specific option objects.

This BEP does not make arbitrary remote jobs durable by itself. It defines the provider/resource contracts required for a workflow engine to persist and resume them.

This BEP does not require every provider to support streaming, tools, background execution, sessions, realtime, or managed caches.

This BEP does not hide semantic differences between vendor APIs. A batch of 10,000 independent requests, one long-running response, and one live duplex session remain different capabilities.

## The mental model

The five nouns are intentionally distinct.

| Noun            | Meaning                                          | Example                                  |
| --------------- | ------------------------------------------------ | ---------------------------------------- |
| LLM function    | A named typed task                               | `ExtractInvoice(document) -> Invoice`    |
| `LlmRequest<T>` | One rendered invocation that has not run         | `ExtractInvoice$request(document)`       |
| Provider        | An object that may implement capabilities        | `OpenAi { model: "..." }`                |
| Capability      | One supported interaction shape                  | `Generate`, `Streaming`, `Background`    |
| Resource        | A live or durable thing returned by an operation | `Job<Invoice>`, `Session`, `LiveSession` |

The usual flow is:

```text
LLM function call
    -> generated LlmRequest<T>
    -> standard capability driver
    -> runtime match on request.provider
    -> provider capability method
    -> LlmResponse<T>, Stream<...>, Job<T>, Session, ...
```

## Decision rules

These are the user-facing rules for choosing an abstraction.

### Rule 1: Start with an LLM function

If the operation has a meaningful typed result and prompt, declare an LLM function even when it will eventually stream, run in the background, use tools, or execute inside a session.

```baml
function ClassifyTicket(ticket: Ticket) -> Classification {
  client: DefaultModel
  prompt: `Classify ${ticket}. ${ctx.output_format}`
}
```

The task should not be rewritten inside provider code.

### Rule 2: Implement `Generate` when the operation is still prompt to typed answer

A new endpoint, local model, gateway, policy wrapper, tracing wrapper, or option preset should implement or wrap `Generate` when its observable shape is still:

```text
LlmRequest<T> -> LlmResponse<T>
```

Application code then calls the original LLM function with `client = custom_provider`.

### Rule 3: Add a capability when the interaction shape or lifecycle changes

Use a separate capability for a distinct protocol such as:

- partial output over time;
- tool-call turns;
- background submission and polling;
- a provider-owned conversation;
- a live duplex channel;
- a managed cache with deletion;
- a batch containing many independent requests.

A vendor option such as temperature is not a capability. A wrapper that adds a header is not a capability. A long-lived session is.

### Rule 4: Return a resource when later operations depend on provider-owned state

If the caller must poll, continue, fork, cancel, close, delete, or resume something, return an object with those methods. Do not make the user carry an ID plus the correct provider plus the correct lifecycle rules independently.

### Rule 5: Use an ordinary driver function for a user-defined capability

A capability driver accepts `LlmRequest<T>`, dynamically checks the provider, and calls the capability method. It does not require compiler registration.

```baml
function run_moderated<T>(request: baml.ai.LlmRequest<T>, policy: Policy) -> T {
  match (request.provider) {
    let p: Moderated => p.generate_moderated<T>(request, policy).value,
    _ => throw baml.errors.Unsupported {
      capability: "example.Moderated",
      provider: request.provider_name(),
    },
  }
}
```

### Rule 6: Call a provider-specific method directly when the operation is not an LLM task

Listing models, uploading a file, deleting a cache, checking provider health, or administering a fine-tune does not need a fake LLM function. Use an ordinary provider API.

When the operation is an LLM task but needs a nonstandard lifecycle, keep the LLM function and pass its `$request` to the provider-specific method.

## Proposed design

### `Provider` is the dynamic root

`Provider` is a marker interface. It does not require a universal request method.

```baml
interface Provider {}
```

Application entrypoints accept `Provider` when they intentionally support dynamic routing. Lower-level helpers accept a narrower capability interface when the capability is required statically.

```baml
function choose_provider(tenant: Tenant) -> baml.ai.Provider

function consume_stream(
  provider: baml.ai.Streaming,
  request: baml.ai.LlmRequest<Report>,
) -> Report
```

### `LlmRequest<T>` is the universal task handoff

The standard library defines the conceptual shape below. Some fields may be compiler/runtime-backed rather than publicly constructible fields, but their behavior is normative.

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
  function provider_name(self) -> string throws never
  function for_provider(self, provider: Provider) -> LlmRequest<T> throws never
}
```

`LlmRequest<T>` carries:

- the selected provider;
- the fully rendered `PromptAst`, including roles and media;
- the output type `T` used to render `${ctx.output_format}` and build native schemas;
- the LLM function's stable identity when one exists;
- captured arguments for traces and debugging;
- portable request options and tags.

It does **not** carry a provider-specific HTTP body. A provider creates its own wire request from the semantic request.

The request privately retains enough of its prompt render recipe to implement `for_provider`. That method does not merely replace a field: it rebuilds provider-sensitive prompt context and re-renders the `PromptAst`. Drivers and wrappers MUST pass a provider a request bound to that provider. This is how round-robin/fallback members and provider wrappers preserve `${ctx.client...}` semantics.

### Every LLM function gets `$request`

For this declaration:

```baml
function ExtractInvoice(document: pdf) -> Invoice {
  client: DefaultModel
  prompt: `Extract ${document}. ${ctx.output_format}`
}
```

the compiler exposes:

```baml
function ExtractInvoice$request(
  document: pdf,
  client: baml.ai.Provider = DefaultModel,
) -> baml.ai.LlmRequest<Invoice>
```

The request companion renders the prompt but performs no provider I/O.

### Prompt templates are lazy

The `prompt` tagged template has the conceptual type:

```baml
type PromptTemplate = (baml.llm.Context) -> baml.llm.PromptAst
```

Therefore this expression is a template, not an already rendered prompt:

```baml
let template = prompt`
  ${role("system")}You extract invoices.
  ${role("user")}Hello ${company}.
  ${ctx.output_format}
`
```

It is lazy because `ctx.output_format` depends on `T`, and provider-sensitive prompt context may depend on the provider selected for this request. `baml.ai.request<T>` evaluates the template with the correct context:

```baml
let request = baml.ai.request<Invoice>(
  provider,
  prompt`Extract ${document}. ${ctx.output_format}`,
)
```

This is the manual equivalent of an LLM function's `$request` companion.

### The baseline capability is semantic generation

`Generate` is transport-independent. It receives the complete semantic request and returns a value plus normalized metadata.

```baml
interface Generate requires Provider {
  function generate<T>(self, request: LlmRequest<T>) -> LlmResponse<T>
    throws baml.errors.GenerateError | baml.errors.UnknownError
}

class LlmResponse<T> {
  value: T,
  meta: ResponseMeta,
}
```

An HTTP provider may build JSON and send HTTP. A local provider may call a host runtime. A test provider may return a fixture. A gateway may delegate to another provider. None of those transport choices belong in the `Generate` capability.

### Standard drivers negotiate capabilities

The standard library owns ordinary driver functions:

```baml
function run<T>(request: LlmRequest<T>) -> T
function run_with_meta<T>(request: LlmRequest<T>) -> LlmResponse<T>
function stream<TPartial, T>(request: LlmRequest<T>) -> baml.llm.Stream<TPartial, T>
function run_tools<T>(request: LlmRequest<T>, tools: Tool[], dispatch: ToolDispatcher)
  -> ToolSucceeded<T> | ToolBudgetReached | ToolHandoff
function submit_background<T>(request: LlmRequest<T>, options: BackgroundOptions) -> Job<T>
function open_live<T>(request: LlmRequest<T>, channel: Channel) -> LiveSession
```

Each driver performs a runtime `match` on `request.provider`. A missing capability throws typed `Unsupported`. Drivers may define explicit, documented degradation paths, but silence is forbidden. For example, `run` may drain a stream only if that behavior is specified and observable; `open_live` may not pretend that an HTTP call is realtime.

### The ordinary LLM function is sugar over `run`

The compiler lowers:

```baml
let invoice = ExtractInvoice(document)
```

as if the user wrote:

```baml
let invoice = baml.ai.run(
  ExtractInvoice$request(document, client = DefaultModel),
)
```

An explicit client override changes only request construction:

```baml
let invoice = ExtractInvoice(document, client = MyGateway)
```

lowers to:

```baml
baml.ai.run(ExtractInvoice$request(document, client = MyGateway))
```

### Streaming remains a generated convenience

Typed partial output depends on the compiler-generated stream form of `T`. Streaming is common enough that the compiler continues to expose `$stream`:

```baml
function ExtractInvoice$stream(
  document: pdf,
  client: baml.ai.Provider = DefaultModel,
) -> baml.llm.Stream<Invoice$stream, Invoice>
```

Its body is equivalent to:

```baml
baml.ai.stream<Invoice$stream, Invoice>(
  ExtractInvoice$request(document, client = client),
)
```

The compiler-generated set is fixed and small: the main function, `$request`, `$stream`, `$render_prompt`, and `$parse`. New user capabilities do not multiply this set.

### Metadata is always produced once

Providers return `LlmResponse<T>` from `Generate`; `run` drops the metadata and `run_with_meta` preserves it. This avoids executing a second request merely to inspect usage or finish reason.

```baml
let response = baml.ai.run_with_meta(
  ExtractInvoice$request(document),
)

log.info(`used ${response.meta.usage?.input_tokens ?? 0} input tokens`)
let invoice = response.value
```

Normalized metadata contains common dimensions and an escape hatch for provider-specific information:

```baml
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

## Capabilities

### A capability describes one interaction shape

A capability interface MUST:

- `require Provider`;
- own methods with one coherent lifecycle;
- accept `LlmRequest<T>` when it executes an LLM task;
- expose a capability-specific typed error channel;
- return a resource when follow-up operations share provider-owned state.

It MUST NOT exist only to represent a vendor flag or a small configuration preset.

### Capability support is both static and dynamic

Use the capability type directly when support is required:

```baml
function require_streaming(
  provider: baml.ai.Streaming,
  request: baml.ai.LlmRequest<Report>,
) -> Report {
  provider.stream<Report$stream, Report>(request).final()
}
```

Use `Provider` plus a runtime match when support is selected dynamically:

```baml
match (request.provider) {
  let p: baml.ai.Streaming => p.stream<Report$stream, Report>(request),
  _ => throw baml.errors.Unsupported {
    capability: "baml.ai.Streaming",
    provider: request.provider_name(),
  },
}
```

Payload-dependent feature support, such as whether one model accepts video or one schema fits a constrained decoder, is reported by an optional descriptor/probe. Implementing an interface means the operation exists; it does not promise every possible payload succeeds.

### User-defined capabilities require no registration

The complete extension sequence is:

1. Declare an interface.
2. Implement it on one or more providers.
3. Write a driver function over `LlmRequest<T>`.
4. Call the driver with an LLM function's `$request`.

```baml
interface Moderated requires baml.ai.Provider {
  function generate_moderated<T>(
    self,
    request: baml.ai.LlmRequest<T>,
    policy: Policy,
  ) -> baml.ai.LlmResponse<T> throws ModerationError
}

function run_moderated<T>(
  request: baml.ai.LlmRequest<T>,
  policy: Policy,
) -> T {
  match (request.provider) {
    let p: Moderated => p.generate_moderated<T>(request, policy).value,
    _ => throw baml.errors.Unsupported {
      capability: "example.Moderated",
      provider: request.provider_name(),
    },
  }
}

let answer = run_moderated(
  Summarize$request(document, client = GuardedVendor),
  StrictPolicy,
)
```

There is no suffix to reserve, no compiler marker, and no global scan. The driver is directly callable, importable, testable, and discoverable like any other BAML function.

## Stateful resources

### A handle is not enough

Returning `{ id, owner }` makes the application reconstruct several invariants:

- which provider can use the ID;
- how to poll or continue it;
- which terminal states exist;
- whether cancellation is supported;
- whether cleanup is required;
- how to serialize it safely;
- how to parse the eventual `T`.

The provider implementation already knows these answers. It should return an object that retains that knowledge.

### Background jobs

```baml
interface Job<T> {
  function status(self) -> JobPhase throws baml.errors.BackgroundError
  function poll(self) -> JobPending | JobSucceeded<T> | JobFailed | JobCancelled
    throws baml.errors.BackgroundError
  function cancel(self) -> JobPhase throws baml.errors.BackgroundError
  function token(self) -> JobToken throws never
  function cleanup(self) -> void
}
```

A provider returns its own implementation, such as `OpenAiResponseJob<T>`. That object contains the provider instance, provider response ID, parser type, last status, and cleanup behavior. Users work through `Job<T>`.

For persistence across processes, `token()` returns a serializable, non-secret token. Resumption is explicit on a configured provider:

```baml
let resumed: baml.ai.Job<Review> = LongRunningModel.resume_job<Review>(saved_token)
```

### Sessions and realtime connections

The same rule applies to `Session` and `LiveSession`:

```baml
interface Session {
  function run<T>(self, request: LlmRequest<T>) -> T
  function run_with_meta<T>(self, request: LlmRequest<T>) -> LlmResponse<T>
  function fork(self) -> Session
  function compact(self, policy: CompactionPolicy) -> CompactionResult
  function token(self) -> SessionToken
  function cleanup(self) -> void
}
```

The object owns the provider-specific continuation identifier and enforces that requests are executed by the owning provider. A realtime resource additionally owns its transport, event loop, interruption state, and close operation.

## Provider composition

### Wrappers are providers

Moderation, tracing, default settings, caching, and policy enforcement are usually wrappers around the same interaction shape.

```baml
class GuardedProvider {
  inner: baml.ai.Generate,
  policy: Policy,

  implements baml.ai.Provider {}

  implements baml.ai.Generate {
    function generate<T>(
      self,
      request: baml.ai.LlmRequest<T>,
    ) -> baml.ai.LlmResponse<T> {
      self.policy.check_input(request.messages())
      let response = self.inner.generate<T>(request.for_provider(self.inner))
      self.policy.check_output(response.value)
      response
    }
  }
}

// No custom capability is needed; the shape is still Generate.
let result = Summarize(document, client = GuardedProvider {
  inner: OpenAiModel,
  policy: StrictPolicy,
})
```

A wrapper only implements capabilities it can correctly forward. A `Generate` wrapper is not automatically `Streaming`, `Background`, or `Realtime`.

### Retry and fallback are operation-aware

The framework does not ask whether an entire provider is “effectful.” Instead, every driver constructs an operation with a replay policy:

```baml
enum ReplayKind {
  Safe,
  RequiresIdempotencyKey,
  Never,
}

class ReplayPolicy {
  kind: ReplayKind,
  idempotency_key: string?,
}
```

Errors report whether the failed attempt is known not to have committed, known to have committed, or has unknown commit state. Retry/fallback may re-drive only when the operation policy and error state permit it. Projection or post-processing failures never re-drive provider I/O.

## Standard library direction

The initial standard library contains these capability families:

| Family             | Capability     | Driver/result                     |
| ------------------ | -------------- | --------------------------------- |
| Immediate          | `Generate`     | `run`, `run_with_meta`            |
| Incremental        | `Streaming`    | `stream`                          |
| Tool loop          | `Tools`        | `run_tools`, typed outcome union  |
| Deferred one-task  | `Background`   | `submit_background`, `Job<T>`     |
| Deferred many-task | `Batching`     | `submit_batch`, `Batch<T>`        |
| Conversation       | `Sessions`     | `open_session`, `Session`         |
| Realtime           | `Realtime`     | `open_live`, `LiveSession`        |
| Managed context    | `ManagedCache` | `create_cache`, `CacheResource`   |
| Inspection         | `Inspectable`  | `build_request`, `RequestPreview` |
| Prompt context     | `PromptInfo`   | provider-aware render metadata    |

HTTP codecs, SSE decoders, WebSocket transports, authentication helpers, SAP parsing, and JSON-schema conversion are library implementation tools. They are not provider capabilities merely because a provider uses them.

See [Standard library surface](./pages/standard-library.md) for intended built-in provider coverage.

## Host SDK behavior

Generated host SDKs keep the common operations ergonomic:

```typescript
const review = await b.ReviewRepository("boundaryml/baml");
const stream = b.stream.ReviewRepository("boundaryml/baml");
```

Advanced operations expose request builders rather than generating one method per capability:

```typescript
const request = b.requests.ReviewRepository("boundaryml/baml", {
  client: longRunningModel,
});
const job = await baml.ai.submitBackground(request, {
  idempotencyKey: "review-boundaryml-baml-v1",
});
```

Custom capability libraries can wrap the generated request type in their own host SDK helpers without modifying BAML code generation.

## Design tradeoffs

### Why not put every method on `Provider`?

It would be convenient for autocomplete, but it would make unsupported operations appear statically valid and would force unrelated lifecycles onto every implementor. Capability interfaces keep the contract honest.

### Why not make providers accept only `PromptAst`?

`PromptAst` preserves prompt structure, but by itself it loses the selected provider, output type, function identity, captured arguments, tags, and operation options. `LlmRequest<T>` contains the `PromptAst` and the missing execution context. Providers can call `request.messages()` when they want the structural message view.

### Why not pass only `ChatMessage[]`?

Messages are the right wire-neutral content representation, but they do not carry `T`, function identity, options, or tracing context. Converting the request to messages should be a method, not a lossy framework boundary.

### Why no global custom capability registry?

An ordinary driver function already provides dynamic negotiation and a typed API. A registry adds suffix naming, whole-program scans, diagnostics, compiler-generated functions, and cross-package ordering without adding expressive power. `$request` is the single compiler hook custom libraries need.

### Why retain `$stream`?

Streaming is common, already part of the expected LLM-function experience, and requires the compiler-derived partial type. Keeping one standard companion is substantially more ergonomic than asking every user to spell both type parameters.

### Why separate background and batch?

A single long-running response and a set of independently keyed requests have different result ordering, cancellation, retry, and persistence semantics. A common “async” flag would erase those differences.

### Why resource objects instead of plain serializable handles?

Resource objects are safer within a process because they retain their owner and lifecycle. Serializable tokens remain available for crossing process boundaries. The two jobs are different and should have different types.

## Rejected alternatives

### One companion per capability

Rejected because the API grows with every combination of LLM function and installed capability. It also makes capability names global compiler concerns.

### A universal `$using(mode)` companion

This is superficially elegant:

```baml
ReviewRepository$using(repo, mode = background(...))
```

However, mode return types depend on the LLM return type (`T`, `Stream<TPartial, T>`, `Job<T>`, `Session`, and custom sums). Expressing this generically requires higher-kinded or generic-associated output types and makes custom mode diagnostics harder to understand. Ordinary driver functions are simpler:

```baml
baml.ai.submit_background(ReviewRepository$request(repo), options)
```

### Provider-wide `is_effectful()`

Rejected because one provider may offer safe immediate generation, provider-stored conversations, background jobs, and realtime sessions. Replay safety belongs to the operation being attempted.

### Provider-specific IDs in public handle classes

Rejected because IDs alone do not encode the owner, parser, lifecycle, or credential boundary.

### A single “supports” map as the capability system

Rejected because a string map cannot provide method signatures or static requirements. Descriptors remain useful for graded or payload-dependent support, but interfaces define callable behavior.

## Security and privacy considerations

`LlmRequest.arguments`, raw response metadata, and resource tokens may contain sensitive data. Tracing and serialization MUST be opt-in at field granularity and MUST honor redaction policies.

Resource tokens MUST NOT contain API keys. They SHOULD contain an opaque provider instance name plus the minimum remote identifier required for resumption. Providers MUST validate token ownership before using a token.

Background and cache capabilities MUST document retention and cleanup behavior. Opening a resource whose provider stores data is an observable policy decision, not a transparent optimization.

Tool drivers MUST preserve tool-call IDs, validate arguments against declared types, distinguish local from server-executed tools, and enforce explicit permissions before executing side effects.

Provider-specific `raw` metadata SHOULD be disabled or redacted by default in production traces.

## Implementation plan

### Phase 1: Introduce `LlmRequest<T>` and `$request`

- Add the semantic request type and manual `baml.ai.request<T>` constructor.
- Generate `$request` for every LLM function.
- Lower the main body and `$stream` through `$request`.
- Keep existing call paths behind adapters during migration.

### Phase 2: Introduce transport-independent `Generate`

- Add `LlmResponse<T>` and normalized `ResponseMeta`.
- Adapt built-in providers to `Generate`.
- Move HTTP codec interfaces to provider implementation helpers.
- Add wrapper-provider examples and fixtures.

### Phase 3: Move standard drivers to requests

- Streaming, tools, metadata, and inspection consume `LlmRequest<T>`.
- Capability absence uses one typed `Unsupported` shape.
- Remove dynamic custom-companion generation after migration warnings exist.

### Phase 4: Introduce owned lifecycle resources

- Implement `Job<T>`, `Batch<T>`, `Session`, `LiveSession`, and `CacheResource` interfaces.
- Add serializable resume tokens and explicit provider resume methods.
- Integrate `cleanup()` behavior and explicit close/delete methods.

### Phase 5: Operation-aware composition

- Add replay policy and commit-state classification.
- Make retry/fallback capability-specific.
- Ensure metadata projection and output post-processing execute exactly once.

### Phase 6: Tooling and host SDKs

- Display generated `$request` signatures in `baml describe`.
- Add request construction to Python and TypeScript codegen.
- Add LSP actions that explain missing capabilities and suggest the narrow interface or driver.
- Add scenario conformance tests for the full matrix.

## Acceptance criteria

The proposal is complete when all of these work:

1. A normal LLM function calls a built-in provider through `Generate`.
2. `$stream` uses the same rendered request and produces the compiler-derived partial type.
3. A user-authored HTTP provider implements `Generate` entirely in BAML.
4. A user-authored wrapper changes policy while plain LLM functions remain unchanged.
5. A user-authored capability and driver operate on any LLM function without compiler registration.
6. A background LLM function returns a typed `Job<T>` that polls, cancels, serializes a token, resumes, and cleans up.
7. A provider with immediate and stateful operations gets different replay policies per operation.
8. Retry/fallback never repeats provider I/O after a projection or local parse callback throws.
9. Runtime capability absence is a typed `Unsupported` error containing provider and capability identities.
10. `baml describe` shows the original LLM function, `$request`, generated standard companions, and the provider interfaces involved.
11. Host SDK users can build a request and pass it to a custom capability library.
12. The scenario corpus has at least one offline test per capability and live tests for each built-in provider implementation.

## Open questions

1. Should `LlmRequest.arguments` be public, trace-only, or exposed through a redacting accessor?
2. Should `ResponseMeta.raw` be `json?`, `unknown?`, or a provider-specific typed sidecar accessible through a separate API?
3. Which portable request options belong directly on `RequestOptions`, and which should always stay provider-specific?
4. Should `run` be allowed to drain `Streaming` when `Generate` is absent, or should baseline generation require `Generate` exactly?
5. What stable provider-instance identifier should serialized resource tokens use?
6. Should a `Job<T>.cleanup()` cancel pending work, merely release local resources, or be provider-configurable? Explicit `cancel()` remains required either way.
7. How much of an `LlmRequest<T>` may cross a host boundary without losing the opaque `PromptAst` representation?

## Additional pages

- [Getting started](./pages/getting-started.md)
- [Writing custom providers and capabilities](./pages/custom-providers.md)
- [Background jobs end to end](./pages/background-jobs.md)
- [Scenario cookbook](./pages/scenario-cookbook.md)
- [Standard library surface](./pages/standard-library.md)
- [Compiler and runtime expansion](./pages/under-the-hood.md)
- [Effects, errors, and testing](./pages/effects-errors-testing.md)
- [Prior art and provider research](./pages/prior-art.md)
- [Changes from the design on this branch](./changes_from_old_design.md)
