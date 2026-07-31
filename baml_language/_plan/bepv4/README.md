# BEP-064: AI functions and agents

Status: proposed.

This BEP defines one normal lifecycle for typed model work: an `ai.run.Agent`
drives an `ai.AgentProvider`. The same lifecycle handles a task with no
application tools, one tool, or many tools.

## Minimal example

```baml
class Resolution {
  category: string,
  summary: string,
  reply: string,
}

/// Search the support knowledge base.
function search_knowledge(query: string) -> json throws never {
  {
    "query": query,
    "article": "Duplicate charges are normally pending authorizations.",
  }
}

function ResolveTicket(ticket: SupportTicket) -> Resolution {
  provider: fast_model()
  prompt: `
    Resolve ticket ${ticket.id}.
    ${ctx.output_format}
  `
  tools: [search_knowledge]
}

let outcome = ResolveTicket@task(sample_ticket()).run(
  runner = ai.run.Agent<Resolution>.new(),
)
```

An explicit Agent run returns:

```baml
ai.Done<Resolution> | ai.Stopped | ai.Handoff | ai.Interrupted | ai.Failed
```

Every variant carries the committed conversation, `steps_taken`, and
cumulative `usage`. `Done<T>` adds the typed value and response metadata.
`Stopped` is a voluntary policy stop — the runner's `max_steps` limit, its
`stop_when` predicate, or a `StepPlan` stop — with resumable state and a
`reason` naming which policy fired. `Handoff` preserves the exact
`ToolCall`; the application must submit a correlated `ai.tools.ToolResult` —
the union `ToolOk | ToolError`, built with `ToolOk.of(call, output)` or
`ToolError.of(call, message)` — before resuming. `Interrupted` is a
cooperative cancellation at the last committed checkpoint. `Failed` is an
involuntary stop after committed progress, carrying its classified `cause`;
a failure before any progress still throws.

Run the corresponding scenario with:

```console
baml run --from crates/baml_tests/baml_src_temp2 \
  ai_scenarios.agent_loop
```

## Direct calls

The concise form remains:

```baml
let resolution: Resolution = ResolveTicket(sample_ticket())
```

The compiler lowers this internally to a default
`ai.run.Agent<Resolution>`, runs it, projects `Done<Resolution>` into
`ResponseWithMetadata<Resolution>`, and exposes its `.value` as the generated
function result. The explicit spelling of this value-unwrap contract is
`task.complete(runner?)`: a stop state — `Stopped`, `Handoff`, or
`Interrupted` — cannot satisfy the declared return type, so it surfaces as
`ai.IncompleteRun` (which still carries the committed outcome, losslessly
and resumably), and a `Failed` outcome rethrows its cause. Direct generated
calls share this exact unwrap. `IncompleteRun` deliberately does NOT
implement `ai.Failure` — it is its own term in every `throws` union that can
carry it (`throws ai.IncompleteRun | ai.Failure | baml.errors.UnknownError`),
so a generic `ai.Failure` catch arm never silently absorbs a resumable stop.

There is no separate value-only or single-turn runner. A task with no
application tools normally finishes after one provider step; a task that
requests tools takes more steps. The Agent lifecycle is the same in both
cases.

## Ownership boundary

The public protocol is intentionally small:

```baml
interface Provider {
  function name(self) -> string throws never
  function render_shorthand(self) -> string throws never
  // Thin wrappers (retry, middleware) return their inner provider; leaf
  // providers keep the default null. Ownership checks walk this chain.
  function delegate(self) -> Provider? throws never
}

class ModelStep<T> {
  outcome: T | ai.tools.ToolCalls,
  metadata: ai.ResponseMetadata,
  // Provider-neutral display channels, when the vendor exposes them.
  assistant_text: string?,
  reasoning_text: string?,
}

interface AgentProvider requires Provider {
  function begin<T>(self, task: ai.Task<T>) -> ai.Conversation
  function step<T>(
    self,
    conversation: ai.Conversation,
    tools: ai.tools.Tool[],
  ) -> ai.ModelStep<T>
  function submit(
    self,
    conversation: ai.Conversation,
    results: ai.tools.ToolResult[],
  ) -> ai.Conversation
}

interface ConversationAppendProvider requires AgentProvider {
  function append_messages(
    self,
    conversation: ai.Conversation,
    messages: ai.Messages,
  ) -> null
}
```

The error clauses are omitted above for readability.

`Provider` is configuration and prompt-rendering identity. Its
`render_shorthand` is a `"vendor/model"` string; a malformed value raises
`ai.InvalidRequest` at first render, and agent runs validate it up front.
Instance identity is built in: `ai.same_provider_instance(a, b)` answers
"same provider instance (or reachable through `delegate()`)" — the rule
every conversation-ownership check uses. `AgentProvider`
adds exactly one model-turn protocol:

- `begin` creates provider-owned conversation state without a model request;
- `step` performs one model request and returns either `T` or tool calls;
- a replay-safe failed `step` leaves its conversation unchanged;
- `submit` validates and records correlated tool results without asking the
  model for another turn.

The provider owns authentication, request rendering, response parsing, wire
IDs, and exact continuation state. It does not execute application tools and
does not own a model/tool loop.

`ConversationAppendProvider` is the local exact-continuation capability.
Applications normally call `conversation.append_message(...)`; the
conversation dynamically dispatches to its owning provider. Appending
MUTATES the conversation in place and is statement-shaped (returns null); a
failing append leaves the conversation unchanged. This preserves opaque
state and makes no model request. It is distinct from
`ConversationImportProvider`, which reconstructs a destination conversation
from portable messages and reports the resulting fidelity.

The Agent owns:

- the loop, the `max_steps` limit, and the caller's `stop_when` policy;
- the active application-tool registry;
- argument validation and `reflect.call_any`;
- approval, replacement, blocking, and handoff policy;
- provider switching through explicit conversation import;
- cumulative usage and lifecycle events.

This boundary prevents a provider from recursively invoking a runner that
invokes the provider again.

## One turn versus one run

These terms are distinct:

| Term | Meaning |
| --- | --- |
| Provider step | One call to `AgentProvider.step` and therefore one model request |
| Agent run | Zero or more provider steps, with application tool execution between steps |
| Direct call | A default Agent run whose successful `Done<T>` is unwrapped |

A provider may internally choose a native result tool, a native JSON schema,
or SAP text parsing for a step. That wire decision does not create another
runner.

## Reliability boundary

`ai.retry(provider, max_attempts, retry_if = null, backoff = ai.Backoff.default())`
and `ai.fallback(providers)` are `AgentProvider` wrappers.

- Retry repeats only the current model step, and only when the failure
  reports `Effects.None` and the judgment accepts it — effect safety is a
  fact the gate always enforces; which effect-safe failures are worth
  replaying is the caller's judgment, expressed through `retry_if` (the
  default declines `Refused`/`InvalidRequest`/`ParseFailed` and replays the
  other effect-safe failures, including `NetworkFailure` — safe because a
  failing step leaves its conversation unchanged). `Backoff` is exponential
  (`initial_ms`, `multiplier`, `max_ms`); a provider `RateLimited.retry_after_ms`
  hint overrides the computed delay. Retry never
  restarts the Agent and never replays an application tool that already ran.
- Fallback may choose another provider only before the first successful model
  turn. Once a provider has made progress, switching requires exporting
  portable messages and importing them into the destination provider.

This is stricter than replaying a whole typed call and is necessary when tools
can have effects.

## Other lifecycles remain separate

Agent is the normal model lifecycle. The following APIs remain separate
because their state machines and result types are materially different:

| Lifecycle | Portable API |
| --- | --- |
| Streaming | `ai.run.Stream` / `ai.StreamingProvider` |
| Background work | `ai.run.Background` / `ai.jobs.BackgroundProvider` |
| Batch work | `ai.run.Batch` / `ai.jobs.BatchProvider` |
| Realtime | `ai.realtime` and `ai.run.VoiceAgent` |
| Transcription | `ai.run.Transcribe` |
| External coding/research harness | `ai.run.Harness` / `ai.harness.Harness` |

These capabilities may share a provider value, but they do not delegate to an
Agent unless their own contract explicitly says so.

## Provider namespaces

Portable orchestration lives under `ai`. Provider namespaces expose
configuration-sized public surfaces:

```text
ai
├── Task<T>, Provider, AgentProvider, ModelStep<T>
├── Conversation, MessageHistory
├── Done<T>, Stopped, Handoff, Interrupted, Failed, IncompleteRun
├── retry(...), fallback(...), Backoff
├── same_provider_instance(...), classify_http(...), output_fingerprint<T>()
├── run.Agent, run.Stream, run.Background, run.Batch, run.Harness
├── run.AgentSession<T>, run.AgentSessionToken, run.SessionBusy, run.SessionMismatch
├── run.Transcribe, run.TranscribeWithMeta, run.VoiceAgent
├── tools, observe, jobs, realtime, transcription, harness, testing
│
openai
├── OpenAIProvider, responses(...)
└── Realtime

anthropic
└── AnthropicProvider, messages(...)

google
├── vertex.Gemini, vertex.gemini(...)
└── ai.Gemini, ai.gemini(...)

claude_code
└── ClaudeCodeCli
```

Provider-specific request builders, schema transforms, continuation classes,
prompt-mode adapters, and Claude Code's JSON-schema envelope are private
implementation details. Applications configure the public provider instead
of constructing those adapters directly.

## Guides

| Subject | Guide |
| --- | --- |
| Normal execution and result types | [Tasks, runners, and results](./pages/tasks-runners-and-results.md) |
| Application tools and Agent outcomes | [Agents and tools](./pages/agents-and-tools.md) |
| Approvals, step limits, and handoffs | [Approvals, limits, and handoffs](./pages/approvals-limits-and-handoffs.md) |
| Runtime tool rosters and MCP | [Dynamic tools and MCP](./pages/dynamic-tools-and-mcp.md) |
| Native schemas, result tools, SAP, arrays, and unions | [Structured outputs and tool calling](./pages/structured-outputs-and-tool-calling.md) |
| Failure classification and effects | [Errors and error handling](./pages/errors-and-error-handling.md) |
| Safe retry, fallback, and routing | [Routing, retry, and fallback](./pages/routing-retry-and-fallback.md) |
| Save, resume, and provider switching | [Conversations and resuming](./pages/conversations-and-resuming.md) |
| Multi-turn continuation, forking, and durable sessions | [Agent sessions](./pages/agent-sessions.md) |
| Streaming, media, and transcription | [Streaming, media, and transcription](./pages/streaming-media-and-transcription.md) |
| Background work, batches, and caches | [Jobs, batches, and caches](./pages/jobs-batches-and-caches.md) |
| Realtime sessions | [Voice and live sessions](./pages/voice-and-live-sessions.md) |
| Fakes, live tests, and events | [Testing and observability](./pages/testing-and-observability.md) |
| External harnesses and custom runners | [Harnesses and custom extensions](./pages/harnesses-and-custom-extensions.md) |
| Writing an `ai.AgentProvider` adapter | [Implement a provider](./pages/implement-a-provider.md) |
| Comparison with other AI frameworks | [Why BAML](./pages/why-baml.md) |
