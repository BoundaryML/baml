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
ai.Done<Resolution> | ai.BudgetReached | ai.Handoff
```

`Done<T>` contains the typed value, response metadata, usage, and the
provider-owned conversation. `BudgetReached` preserves resumable state.
`Handoff` preserves both the conversation and the exact `ToolCall`; the
application must submit a correlated `ToolResult` before resuming.

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
function result. `BudgetReached` or `Handoff` cannot satisfy a direct call's
return type, so they surface as failures at this boundary.

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
}

class ModelStep<T> {
  outcome: T | ai.tools.ToolCalls,
  metadata: ai.ResponseMetadata,
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
```

The error clauses are omitted above for readability.

`Provider` is configuration and prompt-rendering identity. `AgentProvider`
adds exactly one model-turn protocol:

- `begin` creates provider-owned conversation state without a model request;
- `step` performs one model request and returns either `T` or tool calls;
- a replay-safe failed `step` leaves its conversation unchanged;
- `submit` validates and records correlated tool results without asking the
  model for another turn.

The provider owns authentication, request rendering, response parsing, wire
IDs, and exact continuation state. It does not execute application tools and
does not own a model/tool loop.

The Agent owns:

- the loop and step/cost budgets;
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

`ai.retry(provider, max_attempts)` and `ai.fallback(providers)` are
`AgentProvider` wrappers.

- Retry repeats only the current model step when the failure is classified as
  safe to retry. It never restarts the Agent and never replays an application
  tool that already ran.
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
├── Done<T>, BudgetReached, Handoff, Budget
├── retry(...), fallback(...)
├── run.Agent, run.Stream, run.Background, run.Batch, run.Harness
├── tools, observe, jobs, realtime, transcription, harness, testing
│
openai
├── Responses, responses(...)
└── Realtime

anthropic
└── Messages, messages(...)

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
| Native schemas, result tools, SAP, arrays, and unions | [Structured outputs and tool calling](./pages/structured-outputs-and-tool-calling.md) |
| Safe retry, fallback, and routing | [Routing, retry, and fallback](./pages/routing-retry-and-fallback.md) |
| Save, resume, and provider switching | [Conversations and resuming](./pages/conversations-and-resuming.md) |
| Streaming, media, and transcription | [Streaming, media, and transcription](./pages/streaming-media-and-transcription.md) |
| Background work, batches, and caches | [Jobs, batches, and caches](./pages/jobs-batches-and-caches.md) |
| Realtime sessions | [Voice and live sessions](./pages/voice-and-live-sessions.md) |
| Fakes, live tests, and events | [Testing and observability](./pages/testing-and-observability.md) |
| External harnesses and custom runners | [Harnesses and custom extensions](./pages/harnesses-and-custom-extensions.md) |
