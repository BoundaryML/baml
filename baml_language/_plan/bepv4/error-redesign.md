# BEPv4 error redesign: one `ai.Error`, facts not judgments

Replaces the capability-interface taxonomy in `ns_ai/failures/` with a single
concrete error class. Written against the corpus at
`crates/baml_tests/baml_src_temp2/`.

## Principles

1. **Errors carry facts; policy makes judgments.** The error records what
   happened (`kind: RateLimit`, `status_code: 429`, `retry_after_ms`). Whether
   that is *retryable* is decided by the retry/fallback layer, in one place —
   not self-declared by each error type via `is_retryable()`.
2. **The value/error split is the termination contract.** A declared, normal
   termination of a runner's state machine is a **value** (`Done`,
   `BudgetReached`, `Handoff`). Failure to fulfill that contract is an
   **error** (429 after retries, parse failure, tool crash, disconnected
   stream, retry/fallback exhaustion).
3. **No stringly erasure at boundaries.** Wrapping layers preserve the
   underlying error via `cause`/`attempts`, never by interpolating it into a
   prose message on `baml.errors.UnknownError`.

## The types

`ns_ai/failures/error.baml` (replaces `protocol.baml`, `unsupported.baml`,
`reliability/errors.baml`):

```baml
/// What went wrong — a fact about the world, not advice about what to do next.
enum ErrorKind {
  Network,          // connect/reset/DNS; the request may never have arrived
  Timeout,          // deadline elapsed; outcome unobserved
  RateLimit,        // provider said slow down (429); see retry_after_ms
  Auth,             // bad/expired credentials (401/403)
  InvalidRequest,   // provider rejected the request shape (400/404/422)
  PolicyRefusal,    // model or provider refused the content
  InvalidOutput,    // response arrived but failed to parse/validate as T; see raw_output
  Tool,             // a tool invocation failed inside an agent loop
  Unsupported,      // capability or feature this provider does not have
  Canceled,         // a cancel token fired
  Internal,         // provider 5xx, or a foreign error wrapped at a boundary
}

/// Whether the failed attempt may have committed side effects.
enum Effects {
  None,       // failed before anything could commit — replay is safe
  Unknown,    // request was sent; outcome unobserved — replay needs idempotency
  Committed,  // effects definitely happened — never blind-replay
}

class Error {
  kind: ErrorKind,
  message: string,
  effects: Effects,

  provider: string?,      // "openai", "anthropic", "fallback", ...
  model: string?,
  status_code: int?,
  code: string?,          // provider-specific error code, verbatim
  retry_after_ms: int?,   // backoff hint from headers, when present
  raw_output: string?,    // InvalidOutput: the raw model text that failed to parse
  cause: Error?,          // the (normalized) error this one wraps
  attempts: Error[],      // exhaustion: every prior attempt, in order
  data: json,             // foreign payload that resisted normalization

  /// Map an HTTP failure to a classified error. The status→kind table lives
  /// here, once, instead of in every provider.
  function http(
    provider: string, model: string?, status_code: int,
    body: string, retry_after_ms: int?, effects: Effects,
  ) -> Error throws never {
    let kind = if (status_code == 429) { ErrorKind.RateLimit }
      else if (status_code == 401 || status_code == 403) { ErrorKind.Auth }
      else if (status_code == 408) { ErrorKind.Timeout }
      else if (status_code >= 400 && status_code < 500) { ErrorKind.InvalidRequest }
      else { ErrorKind.Internal };
    Error {
      kind: kind,
      message: `${provider} http ${status_code}`,
      effects: effects,
      provider: provider, model: model, status_code: status_code,
      code: null, retry_after_ms: retry_after_ms,
      raw_output: null, cause: null, attempts: [], data: body,
    }
  }

  /// A typed-output parse/validation failure. raw_output is mandatory —
  /// callers log it, repair it, or re-prompt with it.
  function parse(provider: string, model: string?, raw_output: string, data: json)
      -> Error throws never {
    Error {
      kind: ErrorKind.InvalidOutput,
      message: `${provider} output failed to parse as the requested type`,
      effects: Effects.None,
      provider: provider, model: model, status_code: null, code: null,
      retry_after_ms: null, raw_output: raw_output,
      cause: null, attempts: [], data: data,
    }
  }

  /// Normalize a foreign error at a boundary. The ONLY sanctioned way to
  /// produce kind Internal from a caught unknown.
  function wrap(context: string, data: json) -> Error throws never {
    Error {
      kind: ErrorKind.Internal, message: context, effects: Effects.Unknown,
      provider: null, model: null, status_code: null, code: null,
      retry_after_ms: null, raw_output: null, cause: null, attempts: [], data: data,
    }
  }

  function unsupported(message: string) -> Error throws never {
    Error {
      kind: ErrorKind.Unsupported, message: message, effects: Effects.None,
      provider: null, model: null, status_code: null, code: null,
      retry_after_ms: null, raw_output: null, cause: null, attempts: [], data: null,
    }
  }
}
```

Deliberate omissions:

- **No `phase` field.** `kind` already separates the cases anything branches
  on; `kind × phase` admits contradictions (`RateLimit`/`parse`). Stream vs.
  call vs. realtime context is recoverable from where you caught it.
- **No `is_*` methods, no `Failure`/`CallError`/`StreamError`/`ToolError`/
  `RealtimeError` interfaces, no `CannotRetry`, no `FakeCallFailure`.**
  All deleted (see Migration).
- **No associated `type Error` on `Runner`.** The corpus has zero third-party
  error types; a closed concrete class is the honest design. If extensibility
  is ever needed, the escape valve is a one-method interface
  (`function error(self) -> ai.Error`), not a classifier quintet.

## Policy: retryability lives in one function

`ns_ai/reliability/replay.baml` — `ReplayPolicy`/`ReplayKind` (the
provider-side contract) are unchanged; `_may_replay` now reads facts:

```baml
/// Judgment, centralized: is this kind worth another attempt at all?
function _transient(e: root.ai.Error) -> bool throws never {
  match (e.kind) {
    ErrorKind.Network => true,
    ErrorKind.Timeout => true,
    ErrorKind.RateLimit => true,
    ErrorKind.Internal => (e.status_code ?? 500) >= 500,
    _ => false,   // Auth, InvalidRequest, PolicyRefusal, InvalidOutput,
                  // Tool, Unsupported, Canceled: retrying cannot help
  }
}

function _may_replay(policy: ReplayPolicy, e: root.ai.Error) -> bool throws never {
  if (!_transient(e)) { return false; }
  //# Effect-safety: the error's account of THIS attempt...
  match (e.effects) {
    Effects.Committed => return false,
    Effects.Unknown => {
      if (policy.idempotency_key == null) { return false; }
    },
    Effects.None => {},
  }
  //# ...combined with the provider's standing replay contract.
  match (policy.kind) {
    ReplayKind.Safe => true,
    ReplayKind.RequiresIdempotencyKey => policy.idempotency_key != null,
    ReplayKind.Never => false,
  }
}
```

Note `InvalidOutput` is *not* transient here: blind re-send of the same prompt
is a policy decision an application opts into (re-prompt with `raw_output` is
usually better). An app that wants it writes its own loop — the facts are on
the error.

## Exhaustion: rethrow the real error, annotated

Retry and fallback do **not** invent a new error type on exhaustion. They
rethrow the last attempt's error with `attempts` filled in. This is what makes
composition work: an outer fallback wrapping an inner retry still sees
`kind: RateLimit` and can act on it, instead of an opaque "all providers
failed" string (`fallback.baml:38,70` today) or a `CannotRetry` with the
history flattened into prose (`errors.baml`, `retry.baml:59`).

## How the code changes

### Provider protocol (`ns_ai/provider/protocol.baml`)

```baml
interface CompletionProvider requires Provider {
  function complete<T>(self, task: Task<T>) -> ResponseWithMetadata<T>
    throws root.ai.Error
  function replay_policy<T>(self, task: Task<T>) -> ReplayPolicy throws never { ... }
}

interface GenerationProvider requires Provider {
  function generate<T>(self, task: Task<T>) -> ResponseWithMetadata<T>
    throws root.ai.Error
}

interface StreamingProvider requires Provider {
  function stream<TPartial, T>(self, task: Task<T>) -> baml.llm.Stream<TPartial, T>
    throws root.ai.Error
}
```

`| baml.errors.UnknownError` disappears from every signature. This is the
enforcement point: a provider that wants to surface a foreign error must
normalize it (`Error.wrap`) before it crosses the protocol boundary.

### OpenAI client (`ns_openai/ns_internal/client.baml`)

Every `throw baml.errors.UnknownError { message: [...] }` becomes a
constructor call. The two sites that matter most:

```baml
// was: UnknownError { data: body, message: ["openai http " + status] }
if (!http_response.ok()) {
  throw root.ai.Error.http(
    "openai", model, http_response.status_code, body,
    _retry_after_ms(http_response),   // parse Retry-After / x-ratelimit headers
    Effects.None,                     // chat completion: nothing committed
  );
}

// was: UnknownError { data: e, message: ["openai typed parse failed"] }
let value = primitive.parse<T>(body) catch (e) {
  _ => throw root.ai.Error.parse("openai", model, body, e),
};
```

Pre-send failures (`build_request`, `specialize_prompt`) wrap with
`Effects.None`; `baml.http.send` failure wraps with `Effects.Unknown` (the
request may have left). Effectful endpoints (e.g. background jobs) pass
`Effects.Committed`/`Unknown` as appropriate — this replaces the
`LiveEffectCommittedFailure` ad-hoc type in the side-effects scenario.

### Retry (`ns_ai/reliability/retry.baml`)

```baml
implements CompletionProvider for Retry {
  function complete<T>(self, task: Task<T>) -> ResponseWithMetadata<T>
      throws root.ai.Error {
    let provider = match (self.inner) {
      let capability: CompletionProvider => capability,
      _ => throw root.ai.Error.unsupported("retry inner cannot drive"),
    };
    let policy = provider.replay_policy<T>(task.with_provider(self.inner));
    let attempts: root.ai.Error[] = [];
    let attempt = 1;
    while (attempt <= self.max_attempts) {
      let result = provider.complete<T>(task.with_provider(self.inner)) catch (e) {
        let failure: root.ai.Error => {
          if (attempt >= self.max_attempts || !_may_replay(policy, failure)) {
            failure.attempts = attempts;   // history rides along, typed
            throw failure;
          }
          attempts = attempts.concat([failure]);
          attempt = attempt + 1;
          continue;
        },
      };
      return result;
    }
    baml.sys.panic("retry exhausted without returning or throwing")
  }
  ...
}
```

Deltas from today: the `_ => throw e` escape arm is gone (the signature
guarantees classification); `CannotRetry` is gone (a transient-but-unsafe
failure is rethrown as itself — `kind` and `effects` already say why replay
was refused); `retry_after_ms` is available here for backoff when the corpus
grows a sleep primitive.

### Fallback (`ns_ai/reliability/fallback.baml`)

Same shape. The two exhaustion sites become:

```baml
    let attempts: root.ai.Error[] = [];
    for (let member in self.providers()) {
      match (member) {
        let provider: GenerationProvider => {
          let response = provider.generate<T>(task.with_provider(member)) catch (e) {
            let failure: root.ai.Error => {
              if (_may_replay(policy, failure)) {
                attempts = attempts.concat([failure]);
                continue;
              }
              failure.attempts = attempts;
              throw failure;             // non-replayable: surface immediately, typed
            },
          };
          return response;
        },
        _ => {},
      }
    }
    //# Exhausted: rethrow the last real failure with the full history.
    match (attempts.at(attempts.length() - 1)) {
      let last: root.ai.Error => { last.attempts = attempts; throw last; },
      null => throw root.ai.Error.unsupported("no fallback member can generate"),
    }
```

A caller now sees *which* kinds failed across the chain instead of
`"all fallback providers failed"`.

### Fakes (`ns_ai/testing/fakes.baml`)

`FakeCallFailure` and its two implements-blocks are deleted. `FakeFailureMode`
maps to constructed errors:

```baml
match (self.failure_mode) {
  FakeFailureMode.Retryable => throw root.ai.Error {
    kind: ErrorKind.RateLimit, message: "injected retryable fake failure",
    effects: Effects.None, provider: self.provider_name, model: "fake",
    status_code: 429, code: null, retry_after_ms: 10,
    raw_output: null, cause: null, attempts: [], data: null,
  },
  FakeFailureMode.Terminal => throw root.ai.Error {
    kind: ErrorKind.Auth, message: "injected terminal fake failure",
    effects: Effects.None, provider: self.provider_name, model: "fake",
    status_code: 401, code: null, retry_after_ms: null,
    raw_output: null, cause: null, attempts: [], data: null,
  },
  FakeFailureMode.Unknown => throw root.ai.Error.wrap(
    "injected unclassified fake failure", null),
}
```

Crucially the fakes now construct the *same type* real providers throw — the
retry tests finally exercise the same code path as a live 429.

### Tool boundary (`ns_ai/tools/models.baml`)

`invoke_tool` stays `throws never` and the model-facing channel stays a string
— that part is correct. Two changes:

```baml
class ToolResult {
  call: ToolCall,
  output: json,
  error_message: string?,
  failure: root.ai.Error?,   // NEW: application-facing typed channel
  ...
}
```

The catch site builds `Error { kind: ErrorKind.Tool, cause: ..., ... }` and
stores it on the result before stringifying for the model, so
`after_tool_call` observers see the typed failure. `ToolRegistryError`
(add/replace collisions) becomes `Error` with `kind: InvalidRequest` and
`code: "duplicate_tool"` — its `name` field finally surfaces via `message`.

### Values (`ns_ai/tools/agent/outcome.baml`)

Unchanged in kind — this is the value channel and stays one:

```baml
enum BudgetReason { MaxSteps, CostCap, Deadline }

class BudgetReached {
  conversation: Conversation,
  steps_taken: int,
  reason: BudgetReason,      // was: string
}
```

### Runner (`ns_ai/execution/runner.baml`)

```baml
interface Runner<Input> {
  type Output
  function run(self, input: Input) -> Self.Output throws root.ai.Error
}
```

The associated `type Error` is removed. Outcomes (`Done | BudgetReached |
Handoff`) live in `Self.Output` unions; anything thrown is classified by
construction. Domain failures of custom runners travel as
`kind: Internal` + `cause`/`data` until real demand justifies reopening.

## Migration checklist

Delete:
- `ns_ai/failures/protocol.baml` (all five interfaces) → replaced by `error.baml`
- `ns_ai/failures/unsupported.baml` (implements-blocks for `baml.errors.Unsupported`)
- `ns_ai/reliability/errors.baml` (`CannotRetry`)
- `FakeCallFailure` + implements-blocks in `ns_ai/testing/fakes.baml`
- Every `| baml.errors.UnknownError` in `throws` clauses across
  `ns_ai/provider/protocol.baml`, retry/fallback, scenario helpers
- `LiveEffectCommittedFailure` in
  `ns_ai_scenarios/03_routing_and_reliability/tests/side_effects_and_idempotency.baml`
  → `Error { effects: Effects.Committed, ... }`

Grep gates (should end at zero in `ns_ai*`/provider namespaces):
- `throw baml.errors.UnknownError` — only `Error.wrap` may absorb foreign errors
- `is_retryable|is_effectful|is_policy_refusal|is_resumable|is_unsupported|is_network_error|is_rate_limit|is_parse_error`
- `root.ai.CallError|StreamError|ToolError|RealtimeError|CannotRetry`

Docs:
- `routing-retry-and-fallback.md`: rewrite around `kind`/`effects`/`attempts`;
  the "FastModel returned rate_limit; replay is safe" promise is now true.
- `tasks-runners-and-results.md`: add the termination-contract rule with
  `BudgetReached` (value) and provider-429-after-retries (error) as the two
  worked examples.

## Borrowed from the Vercel AI SDK (verified against ai@6 source)

Their model: ~35 closed concrete classes off one `AISDKError` base,
`isInstance` discrimination, `APICallError.isRetryable` baked at construction
(408/409/429/5xx default), retry loop with exponential backoff that respects
`retry-after` headers, `RetryError { reason, lastError, errors[] }` wrapper on
exhaustion, `NoObjectGeneratedError { text, response, usage, finishReason }`
for parse failures, tool failures as `tool-error` stream parts (values). No
effect-safety concept.

Adopt:

1. **`meta: ResponseMetadata?` on `Error`**, populated for `InvalidOutput` and
   any post-response failure — a failed parse still cost tokens, and
   `finish_reason == "length"` diagnoses truncated JSON.
2. **Retry-after sanity guard** in the backoff (when the corpus grows one):
   honor `retry_after_ms` only when below a cap (SDK uses 60s) or below the
   computed exponential delay.

Deliberately not adopted: the exhaustion wrapper (breaks kind-visibility for
composed providers — we rethrow with `attempts`), and construction-time
`isRetryable` (judgment belongs to policy, not the error).

## To verify against the compiler (before committing to the design)

1. **Recursive class fields**: `cause: Error?` and `attempts: Error[]` on
   `Error` itself.
2. **Static-style constructors on classes** at this scale (corpus precedent:
   `ToolResult.ok/error`).
3. **Catch with a single typed arm and no `_` wildcard** when the callee's
   `throws` is exactly `root.ai.Error`.
4. **Field mutation on a caught error** (`failure.attempts = attempts`) —
   corpus precedent: `self.calls = self.calls + 1`.
5. Whether `Error` as a name in `ns_ai` collides with anything
   (`Self.Error` resolution in interfaces, `baml.errors.*`); fall back to
   `AiError`/`CallFailure` only if it does.
