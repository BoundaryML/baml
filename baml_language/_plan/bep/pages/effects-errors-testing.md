# Effects, Errors, and Testing

Provider frameworks fail in expensive ways when retry, fallback, output parsing, and external state are treated as one undifferentiated exception path. This page defines the safety model.

## Three separate questions

For every failed operation, the framework must answer:

1. **What kind of failure occurred?** Transport, rate limit, provider refusal, invalid request, parse failure, cancellation, etc.
2. **Did the attempted operation commit?** Definitely not, definitely yes, or unknown.
3. **May this operation be replayed?** Safely, only with an idempotency key, or never.

No single provider-wide `is_effectful` flag can answer all three.

## Operation replay policy

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

Examples:

| Operation                 | Typical policy           | Why                                                                      |
| ------------------------- | ------------------------ | ------------------------------------------------------------------------ |
| Immediate generation      | `Safe`                   | Repeating may add cost but normally creates no durable application state |
| Stream before first chunk | `Safe`                   | No output has been observed yet                                          |
| Stream after first chunk  | `Never` by default       | Replaying would splice two generations                                   |
| Background submit         | `RequiresIdempotencyKey` | A timeout may occur after job creation                                   |
| Background poll           | `Safe`                   | Read-only status retrieval                                               |
| Session turn              | key or `Never`           | Repeating may append the same turn twice                                 |
| Session fork              | key or `Never`           | Creates durable provider state                                           |
| Realtime open             | `Never`                  | Opens a live connection/session                                          |
| Cache create              | key or `Never`           | Creates billable stored state                                            |
| Cache delete              | provider-declared        | Often idempotent, but not assumed                                        |
| Local metadata projection | not provider I/O         | It must execute once and must never trigger replay                       |

“Safe” does not mean free. It means the framework is allowed to repeat the semantic operation under the configured retry policy. Tracing still records every attempt and cost.

## Commit state on errors

```baml
enum CommitState {
  NotCommitted,
  Committed,
  Unknown,
}

interface OperationError {
  function retryable(self) -> bool throws never
  function commit_state(self) -> CommitState throws never
}
```

Examples:

- DNS failure before connecting: usually `NotCommitted`.
- HTTP 429 before processing: usually `NotCommitted`.
- connection reset after sending a background request: `Unknown`.
- provider returns a completed response that fails local SAP parsing: `Committed`.
- a local metadata callback throws after a successful response: `Committed`.

Provider implementations classify what they can prove. Unknown is preferable to an unsafe guess.

## Retry decision

Conceptually:

```baml
function may_retry(policy: ReplayPolicy, error: OperationError) -> bool {
  if (!error.retryable()) { return false }

  match (error.commit_state()) {
    CommitState.NotCommitted => policy.kind != ReplayKind.Never,
    CommitState.Committed => false,
    CommitState.Unknown => {
      policy.kind == ReplayKind.RequiresIdempotencyKey
        && policy.idempotency_key != null
    },
  }
}
```

The provider must actually honor the idempotency key. Merely attaching a caller string to an in-memory request does not make remote submission idempotent.

## Fallback decision

Fallback has a stricter constraint: an idempotency key understood by provider A usually has no meaning at provider B.

Therefore:

- fallback is allowed after `NotCommitted` failures when the operation permits replay;
- fallback is not allowed after `Committed`;
- fallback after `Unknown` is disabled unless the application explicitly supplies a cross-provider deduplication strategy;
- streaming fallback is allowed only before observable output by default;
- a session or job resource never silently changes owner.

## Separate provider execution from local projection

Bad structure:

```baml
retry {
  let response = provider.generate<T>(request)
  project(response.meta) // a throw here repeats the model call
}
```

Required structure:

```baml
let response = retry_provider_io(() -> { provider.generate<T>(request) })
let projected = project(response.meta)
```

The same rule applies to:

- user metadata callbacks;
- writing a result to a database;
- policy checks after generation;
- rendering a UI;
- converting the typed result to another local type.

Wrappers that intentionally retry post-processing must do so separately and never re-drive the provider implicitly.

## Error families

Each capability family owns a stable error interface:

```baml
interface GenerateError requires OperationError {
  function is_rate_limit(self) -> bool throws never
  function is_refusal(self) -> bool throws never
  function is_parse_error(self) -> bool throws never
}

interface BackgroundError requires OperationError {
  function phase(self) -> JobPhase? throws never
}
```

Concrete provider errors implement one or more interfaces:

```baml
class AcmeRateLimit {
  retry_after: baml.time.Duration?,

  implements baml.errors.GenerateError {
    function retryable(self) -> bool { true }
    function commit_state(self) -> baml.ai.CommitState { baml.ai.CommitState.NotCommitted }
    function is_rate_limit(self) -> bool { true }
    function is_refusal(self) -> bool { false }
    function is_parse_error(self) -> bool { false }
  }
}
```

The same transport error may implement `GenerateError` and `StreamError` when it can occur on both paths.

## Unknown errors

Foreign host values, unmapped wire failures, and internal bugs still need an escape hatch:

```baml
class baml.errors.UnknownError {
  data: unknown,
  message: string[],
}
```

Providers SHOULD classify expected operational failures. `UnknownError` must not become the only error returned because doing so makes safe composition impossible.

Breadcrumbs add context without replacing an already classified error.

## Unsupported capability versus unsupported payload

These are different errors.

```text
UnsupportedCapability:
  provider AcmeText does not implement Streaming

UnsupportedPayload:
  provider AcmeText implements Generate, but this request contains an image
```

The first is discovered by interface negotiation. The second is discovered by request validation or the provider.

`Unsupported` includes:

```baml
class Unsupported {
  provider: string,
  capability: string,
  detail: string?,
}

class UnsupportedPayload {
  provider: string,
  feature: string,
  detail: string,

  implements GenerateError {
    function retryable(self) -> bool { false }
    function commit_state(self) -> baml.ai.CommitState { baml.ai.CommitState.NotCommitted }
    function is_rate_limit(self) -> bool { false }
    function is_refusal(self) -> bool { false }
    function is_parse_error(self) -> bool { false }
  }
}
```

Payload errors SHOULD identify the unsupported message part, option, schema property, or model constraint.

## Refusals are not parse failures

A provider may return HTTP success but decline to produce `T`. That is a typed refusal with provider metadata, not malformed JSON.

```baml
class Refused {
  category: string,
  explanation: string?,
  meta: ResponseMeta,
}
```

Applications can then branch deliberately:

```baml
Task(input) catch (e) {
  let refused: baml.errors.Refused => handle_refusal(refused),
  let rate: baml.errors.RateLimit => schedule_retry(rate.retry_after),
  let parse: baml.errors.ParseFailure => report_provider_output(parse),
  _ => throw e,
}
```

## Expected control outcomes are values

Tool budget exhaustion, handoff, suspended workflows, and a pending job are expected states:

```text
run_tools<T>(...) -> ToolSucceeded<T> | ToolBudgetReached | ToolHandoff
Job<T>.poll()     -> JobPending | JobSucceeded<T> | JobFailed | JobCancelled
```

The public methods spell the generic unions inline because BAML does not currently have generic type aliases.

Use exceptions for failures to perform an operation, not for every non-`T` branch of a protocol.

## Resource ownership errors

Resuming or using a resource with the wrong provider returns a specific error:

```baml
class WrongResourceOwner {
  resource_kind: string,
  expected: string,
  actual: string,
}
```

Provider IDs are not portable by coincidence. The resource or resume method validates ownership before sending network traffic.

## Testing layers

### Layer 1: Pure request rendering

Test `$request` and `$render_prompt` without network calls:

```baml
test "invoice prompt preserves role and schema" {
  let request = ExtractInvoice$request(sample_pdf, client = FakeGenerate {})
  let messages = request.messages()
  assert.equal(messages.at(0)?.role, "system")
  assert.contains(messages.at(1)?.text() ?? "", "vendor")
}
```

### Layer 2: Driver negotiation

Use tiny fake providers to prove every match arm and `Unsupported` path.

```baml
test "background rejects generate-only provider" {
  let result = baml.ai.submit_background(
    Task$request(input, client = GenerateOnlyFake {}),
    options,
  ) catch (e) {
    let _: baml.errors.Unsupported => "unsupported",
    _ => "wrong",
  }
  assert.equal(result, "unsupported")
}
```

### Layer 3: Provider wire tests

Use a local HTTP server or injected transport to assert:

- roles and media encode correctly;
- native schema is correct;
- auth and idempotency headers are present;
- error responses classify correctly;
- response metadata normalizes correctly;
- SAP parses `T`.

### Layer 4: Wrapper invariants

Count calls. Prove that:

- retry stops at the configured limit;
- fallback chooses the expected member;
- a post-processing throw does not increase provider call count;
- streaming never switches providers after the first chunk;
- a guarded wrapper covers every forwarded capability;
- traces contain every attempt.

### Layer 5: Resource state machines

Test every valid transition and representative invalid transitions:

```text
Queued -> Running -> Succeeded
Queued -> Cancelling -> Cancelled
Running -> Failed
Running -> Expired
Succeeded -> cancel (no duplicate state mutation)
```

Round-trip tokens, reject wrong owners, and verify cleanup executes once.

### Layer 6: Live provider contract tests

Live tests are opt-in and inexpensive. Each provider capability has at least one real API smoke test with explicit model/environment requirements.

Live tests should assert semantic invariants rather than exact model prose:

- result parses as `T`;
- stream yields at least one valid partial and final;
- tool call IDs round-trip;
- background job reaches a terminal state;
- session remembers a nonce;
- cancellation returns a documented terminal/intermediate state.

## Scenario matrix

Every capability test fixture SHOULD include:

| Dimension        | Cases                                              |
| ---------------- | -------------------------------------------------- |
| Provider support | supported, missing capability, payload unsupported |
| Result           | success, typed provider error, unknown error       |
| Replay           | not committed, committed, unknown commit           |
| Wrapper          | direct, retry, fallback, traced                    |
| Output           | string, structured class, union, nullable          |
| Prompt           | roles, interpolation, media, output format         |
| Lifecycle        | create, use/poll, cancel/close, cleanup, resume    |

The matrix is more valuable than one happy-path example per provider class.
