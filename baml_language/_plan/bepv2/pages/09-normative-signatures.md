# 9. Normative Signatures

The exact provider-layer contracts, in one place. Prose on pages 1–8 yields
to this page. The follow-on workflow layer owns its separate normative
contracts on [page 10](./10-workflows.md). Each item below is labeled with
what it requires:

- **[lang]** — already expressible in the language today (interfaces +
  `requires`, default methods, generics, `match` narrowing, associated
  types, spread, `defer`/magic `cleanup()`, function types, `throws`
  unions).
- **[syntax]** — new compiler surface (parser/lowering).
- **[runtime]** — new runtime representation.
- **[stdlib]** — new or changed stdlib API (BAML code in `baml.ai` /
  `baml.errors`).
- **[sdk]** — host SDK code generation work.

## Task surface

### Companion selectors — [syntax] [sdk]

```text
Task.stream    Task.with_meta    Task.background    Task.agent
Task.request   Task.prompt       Task.parse
```

Resolution, capture, and error rules: page 3 "Selector semantics". Each
selector is a generated sibling function with internal name `Task$<mod>`.
The set is closed; growth is a BEP-level event.

### The `tools:` task field — [syntax]

```baml
function Name(args...) -> T {
  client: <provider expression>      // extended field grammar: [syntax]
  tools: <expression of type Tool[]> // optional; presence changes lowering per page 5 matrix
  prompt: <template>
}
```

## Request

### `Request<T>` — [runtime] [stdlib]

```baml
class Request<T> {
  provider: Provider,
  prompt: baml.llm.PromptAst,
  identity: TaskIdentity?,
  arguments: map<string, unknown>,
  tools: Tool[],
  options: RequestOptions,
  tags: map<string, string>,
  _render: PromptRenderRecipe,        // runtime-private: template + captured args

  function messages(self) -> ChatMessage[] throws never
  function output_type(self) -> type throws never
  function provider_name(self) -> string throws never
  function for_provider(self, provider: Provider) -> Request<T> throws never
}

class TaskIdentity { name: string, package: string }
```

`for_provider` re-renders via `_render`; it never merely swaps the field.
`Request<T>` is process-local and not serializable. Manual constructor:

```baml
function baml.ai.request<T>(
  provider: Provider,
  template: (baml.llm.Context) -> baml.llm.PromptAst,
) -> Request<T> throws never
```

## Baseline capabilities

### `Generate` — [stdlib]

```baml
interface Generate requires Provider {
  function generate<T>(self, request: Request<T>) -> Response<T>
    throws baml.errors.CallError | baml.errors.UnknownError
}

class Response<T> {
  value: T,
  meta: Meta,
}

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

### `Streaming` — [stdlib]

```baml
interface Streaming requires Provider {
  function stream<TPartial, T>(self, request: Request<T>) -> baml.llm.Stream<TPartial, T>
    throws baml.errors.StreamError | baml.errors.UnknownError
}
```

`TPartial` is supplied by the `.stream` selector (compiler-derived);
hand-written callers must spell it.

### `Tools` — [stdlib]

```baml
interface Tools requires Provider {
  type Transcript
  function begin<T>(self, request: Request<T>) -> Transcript
    throws baml.errors.ToolError | baml.errors.UnknownError
  function step<T>(self, t: Transcript) -> T | ToolCalls
    throws baml.errors.ToolError | baml.errors.UnknownError
  function submit(self, t: Transcript, results: ToolResult[]) -> Transcript
    throws baml.errors.ToolError | baml.errors.UnknownError
}

class Tool {
  name: string,
  description: string,
  parameters: type,
  handoff: bool?,
  // with BEP-062: handler: erased (A) -> string, enabling baml.ai.tool(fn)
}
class ToolCall  { id: string, name: string, args: string }
class ToolResult { id: string, output: string }
class ToolCalls { calls: ToolCall[] }
```

Loop driver and outcomes (consumed by `.agent` and by plain calls on tool
tasks, per the page 5 matrix):

```baml
class StepInfo { steps_taken: int, cost_usd: float?, calls: ToolCall[] }
class Budget   { max_steps: int?, stop_when: ((StepInfo) -> bool throws never)? }

class Done<T>        { value: T, meta: Meta }
class BudgetReached  { transcript: ChatMessage[], steps_taken: int }
class Handoff        { to: string, args: string, steps_taken: int }

function baml.ai.run_agent<T>(request: Request<T>, budget: Budget? = null)
  -> Done<T> | BudgetReached | Handoff
  throws baml.errors.ToolError | baml.errors.UnknownError
```

### `Background` — [stdlib]

```baml
class BackgroundOptions { idempotency_key: string? }

interface Background requires Provider {
  function submit<T>(self, request: Request<T>, options: BackgroundOptions) -> Job<T>
    throws baml.errors.BackgroundError | baml.errors.UnknownError
  function resume_job<T>(self, token: JobToken) -> Job<T>
    throws baml.errors.BackgroundError | baml.errors.UnknownError
}
```

### `Batching` — [stdlib]

```baml
interface Batching requires Provider {
  function submit_batch<T>(
    self,
    requests: Request<T>[],
    key_of: (Request<T>, int) -> string throws never,
  ) -> Batch<T> throws baml.errors.BackgroundError | baml.errors.UnknownError
}
```

### `Sessions` — [stdlib]

```baml
class SessionOptions {}

interface Sessions requires Provider {
  function open_session(self, options: SessionOptions) -> Session
    throws baml.errors.SessionError | baml.errors.UnknownError
  function resume_session(self, token: SessionToken) -> Session
    throws baml.errors.SessionError | baml.errors.UnknownError
}
```

### `Realtime` — [stdlib]

```baml
interface Realtime requires Provider {
  function open_live<T>(self, request: Request<T>, channel: Channel) -> Live
    throws baml.errors.RealtimeError | baml.errors.UnknownError
}
```

### `ManagedCache` — [stdlib]

```baml
class CacheOptions { ttl: baml.time.Duration }

interface ManagedCache requires Provider {
  function create_cache(self, content: ChatMessage[], options: CacheOptions) -> Cache
    throws baml.errors.UnknownError
}
```

## Resources

All resources implement magic `cleanup()` (at-most-once). Tokens are
serializable, non-secret, and validated by the resuming provider.

### `Job<T>` — [stdlib]

```baml
class Pending   { retry_after: baml.time.Duration? }
class Failed    { error: baml.errors.Failure }
class Cancelled {}

interface Job<T> {
  function poll(self) -> Done<T> | Pending | Failed | Cancelled
    throws baml.errors.BackgroundError | baml.errors.UnknownError
  function cancel(self) -> void
    throws baml.errors.BackgroundError | baml.errors.UnknownError
  function token(self) -> JobToken throws never
  function cleanup(self) -> void
}
```

### `Session` and refinements — [stdlib]

```baml
interface Session {
  function run<T>(self, request: Request<T>) -> T
    throws baml.errors.SessionError | baml.errors.UnknownError
  function run_with_meta<T>(self, request: Request<T>) -> Response<T>
    throws baml.errors.SessionError | baml.errors.UnknownError
  function provider(self) -> Provider throws never
  function token(self) -> SessionToken throws never
  function cleanup(self) -> void
}

interface ForkableSession requires Session {
  function fork(self) -> ForkableSession
    throws baml.errors.SessionError | baml.errors.UnknownError
}

interface CompactableSession requires Session {
  function compact(self, policy: CompactionPolicy) -> CompactionResult
    throws baml.errors.SessionError | baml.errors.UnknownError
}
```

`run`/`run_with_meta` rebind unconditionally:
`self.run(request)` ≡ `... (request.for_provider(self.provider()))`.

### `Live` — [stdlib]

```baml
interface Live {
  function events(self) -> LiveEventStream throws never
  function cancel_response(self) -> void
    throws baml.errors.RealtimeError | baml.errors.UnknownError
  function truncate_assistant_audio(self, played_ms: int) -> void
    throws baml.errors.RealtimeError | baml.errors.UnknownError
  function close(self) -> void
  function cleanup(self) -> void
}
```

### `Cache` — [stdlib]

```baml
interface Cache {
  function run<T>(self, request: Request<T>) -> T
    throws baml.errors.CallError | baml.errors.UnknownError
  function delete(self) -> void throws baml.errors.UnknownError
  function cleanup(self) -> void
}
```

## Errors and replay

### `Failure` — [stdlib]

```baml
enum FailureKind { Transport, RateLimit, InvalidRequest, Refusal, Parse, Unsupported, Cancelled }
enum CommitState { NotCommitted, Committed, Unknown }

interface baml.errors.Failure {
  function kind(self) -> FailureKind throws never
  function commit_state(self) -> CommitState throws never
  function retry_after(self) -> baml.time.Duration? throws never { null }
  function is_resumable(self) -> bool throws never { false }
}

interface baml.errors.CallError     requires Failure {}
interface baml.errors.StreamError   requires Failure {}
interface baml.errors.ToolError     requires Failure {}
interface baml.errors.SessionError  requires Failure {}
interface baml.errors.RealtimeError requires Failure {}
interface baml.errors.BackgroundError requires Failure {
  function is_terminal(self) -> bool throws never
}
```

`UnknownError` remains the unclassified escape hatch and implements
nothing; consumers must not assume anything about it.

### `ReplayPolicy` and `may_replay` — [stdlib]

```baml
enum ReplayKind { Safe, RequiresIdempotencyKey, Never }
class ReplayPolicy { kind: ReplayKind, idempotency_key: string? }

function baml.ai.may_replay(policy: ReplayPolicy, failure: baml.errors.Failure) -> bool
```

Semantics as page 8: `InvalidRequest`/`Refusal`/`Unsupported`/`Cancelled`
never replay; `Safe` replays otherwise; `RequiresIdempotencyKey` replays
only with a key and `commit_state != Committed`; `Never` never replays.
`may_replay` is the sole decision point; wrappers MUST use it.

## Free functions (the negotiation layer) — [stdlib]

```baml
function run<T>(request: Request<T>) -> T
function run_with_meta<T>(request: Request<T>) -> Response<T>
function stream<TPartial, T>(request: Request<T>) -> baml.llm.Stream<TPartial, T>
function run_agent<T>(request: Request<T>, budget: Budget? = null) -> Done<T> | BudgetReached | Handoff
function submit_background<T>(request: Request<T>, options: BackgroundOptions) -> Job<T>
function submit_batch<T>(provider: Provider, requests: Request<T>[], key_of: ...) -> Batch<T>
function open_session(provider: Provider, options: SessionOptions) -> Session
function open_live<T>(request: Request<T>, channel: Channel) -> Live
function create_cache(provider: Provider, content: ChatMessage[], options: CacheOptions) -> Cache
function request<T>(provider: Provider, template: (Context) -> PromptAst) -> Request<T>
function retry(inner: Provider, policy: RetryPolicy) -> Provider
function fallback(members: Provider[]) -> Provider
function traced(inner: Provider, meter: UsageMeter) -> Provider
function may_replay(policy: ReplayPolicy, failure: Failure) -> bool
```

Throws clauses follow the corresponding capability. Each performs one
runtime `match` on the provider and throws typed
`baml.errors.Unsupported` (kind `Unsupported`) on absence.

## Fluent composition sugar — [stdlib]

`Provider` remains an empty marker. Standard dot-call composition is supplied
by an out-of-body blanket implementation over concrete providers:

```baml
interface ProviderFluent requires Provider {
  function with_retry(self, policy: RetryPolicy) -> Retry {
    Retry { inner: self, policy: policy }
  }

  function fallback_to(self, other: Provider) -> Fallback {
    Fallback { members: [self, other] }
  }

  function traced(self, meter: UsageMeter) -> Traced {
    Traced { inner: self, meter: meter }
  }
}

implements<T extends Provider> ProviderFluent for T {}
```

Normative rules:

- `ProviderFluent` is syntax-only, not a capability and not a negotiation
  target.
- Each method MUST be a thin spelling of the corresponding standard wrapper;
  the free functions remain the canonical existential surface.
- Methods return concrete wrapper types so further blanket-provided sugar can
  chain without premature erasure.
- The blanket implementation applies to concrete `T extends Provider`; a value
  statically typed as existential `Provider` uses `retry`, `fallback`, or
  `traced` instead.
- Libraries MAY define separate fluent interfaces and blanket implementations
  for their own wrapper policies. They MUST NOT reopen or add scenario-specific
  methods to `Provider` or `ProviderFluent`.
- Application/business routing remains ordinary code and MUST NOT be encoded as
  standard provider sugar.

Page 8 gives the extension pattern, including library-owned `.judged_by(...)`
sugar and the concrete-versus-existential boundary.

## Composition values — [stdlib]

```baml
class Agent {
  inner: Provider,
  tools: Tool[],
  dispatch: ((ToolCall[]) -> ToolResult[] throws never)?,   // default: schema-validated auto-dispatch
  stop_when: ((StepInfo) -> bool throws never)?,
  implements Provider {}
  implements Generate { ... }        // runs the loop, graceful finish
}
```

Retry/fallback/traced wrappers: existential `inner`, per-capability
forwarding, `may_replay` consulted; page 8.

## Feature-requirement summary

| Contract | lang | syntax | runtime | stdlib | sdk |
| --- | :-: | :-: | :-: | :-: | :-: |
| Companion selectors | | ✓ | | | ✓ |
| `tools:` / `client:` expr fields | | ✓ | | | |
| `Request<T>` + render recipe | | | ✓ | ✓ | ✓ |
| `Generate`/`Response`/`Meta` | ✓ | | | ✓ | |
| `Streaming` over requests | ✓ | | | ✓ | ✓ |
| `Tools` + agent loop + outcomes | ✓ | | | ✓ | ✓ |
| `Background`/`Job`/tokens | ✓ | | ✓ (token identity) | ✓ | ✓ |
| `Batching`/`Sessions`/`Realtime`/`ManagedCache` | ✓ | | | ✓ | |
| Session refinements | ✓ | | | ✓ | |
| `Failure`/`CommitState` | ✓ | | | ✓ | |
| `ReplayPolicy`/`may_replay` | ✓ | | | ✓ | |
| Fluent provider sugar (out-of-body blanket impl) | ✓ | | | ✓ | |
| Provider spread-derivation | ✓ | | | | |
| `baml.ai.tool(fn)` (function-typed tools) | BEP-062 | | | ✓ | |

Everything in the **lang** column exists today; the design's novel load is
concentrated in two compiler features (selectors, task fields), one runtime
representation (the request), and stdlib surface.
