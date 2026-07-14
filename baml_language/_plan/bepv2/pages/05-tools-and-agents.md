# 5. Tools, Messages, Transcripts, and Agents

An agent run is a `Task<T>`, a changing tool roster, a transcript, and an
execution policy. `ai.drivers.run_agent` owns that loop. The task says
what the model should accomplish; it does not hard-code the lifecycle.

## Tools and their owners

Application tools are ordinary typed handlers:

```baml
class WeatherArgs { city: string, days: int }

let weather = ai.tool(
  "get_weather",
  "Get a forecast.",
  (args: WeatherArgs) -> string { forecast(args.city, args.days) },
)
```

Task-owned tools are stable defaults:

```baml
function ResearchQuestion(q: string) -> Answer {
  provider: ToolModel
  tools: [search, calculator]
  prompt: `Research ${q}. ${ctx.output_format}`
}
```

Provider-owned tools—vendor web search, code execution, retrieval—remain
typed provider configuration because the vendor executes them. Driver-owned
tools are supplied by run options, hooks, or a live `ToolRegistry`. The
effective roster is assembled at each step as page 3 specifies.

Application and provider-owned tools are separate authority domains.
`StepPlan.tools: []` can guarantee that the application driver advertises no
application tools. It does not secretly rewrite provider configuration. To
guarantee a turn has no tools of either kind, also select a provider value
whose provider-owned tool list is empty. This explicit provider switch is
observable and preserves typed vendor configuration.

Argument validation happens before dispatch. Invalid arguments become a tool
error result the model can repair unless policy explicitly makes them fatal.
Page 11 specifies how BEP-062 function values remove most hand-written tool
dispatch boilerplate while retaining this runtime `Tool` boundary.

## Messages are an interface

Providers have different content blocks, but drivers need a common structural
view that preserves roles and media. A fixed `ChatMessage` data class is too
narrow, so the public boundary is interfaces:

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
```

The standard library supplies concrete serializable `ConversationMessage` and
`Conversation` classes implementing these interfaces. Providers may expose
richer private message implementations. `provider_metadata` is observable and
round-trippable when untouched; application code should not depend on its
shape.

## Transcript is an interface, not a message array

A transcript is the provider adapter's exact continuation state:

```baml
interface Transcript {
  function provider(self) -> Provider throws never
  function messages(self) -> Messages throws never
  function conversation(self) -> Conversation throws never
}
```

It may contain tool-call IDs, Anthropic thinking signatures, OpenAI reasoning
state, encrypted/redacted blocks, citations, server-side continuation IDs, or
other invariants that are not safe for application code to reconstruct.
`messages()` is an observability/rendering view. `conversation()` is an
editable, serializable, provider-neutral projection. Neither is automatically
the mutable source of truth for the next request.

This follows one ownership rule:

```text
application owns: tool execution, UI, logs, business state, Conversation
provider owns:    exact wire history, signatures, opaque blocks, continuation
driver owns:      current Transcript and active provider during a run
```

## Exact persistence and restoration

Providers that support durable continuation implement:

```baml
class TranscriptToken {
  provider: string,
  version: int,
  sealed: string,
}

interface ResumableToolCallingProvider requires ToolCallingProvider {
  function save_transcript(self, transcript: Transcript) -> TranscriptToken
  function restore_transcript(self, token: TranscriptToken) -> Transcript
}
```

The token is provider-controlled, opaque, serializable, and non-secret unless
the provider documents otherwise. Applications store it but do not decode,
edit, compact, or synthesize it. Restoring with the owning provider preserves
private continuation state exactly.

## Converting between transcripts

Cross-provider conversion cannot promise exactness. The target provider may
import the source's neutral conversation:

```baml
enum TranscriptFidelity { Exact, MessagesOnly, Lossy }

class TranscriptImport {
  transcript: Transcript,
  fidelity: TranscriptFidelity,
  warnings: string[],
}

interface TranscriptImportProvider requires ToolCallingProvider {
  function import_conversation(
    self,
    conversation: Conversation,
  ) -> TranscriptImport throws baml.errors.TranscriptError
}

function ai.transcripts.convert(
  source: Transcript,
  target: TranscriptImportProvider,
) -> TranscriptImport {
  target.import_conversation(source.conversation())
}
```

Rules:

- Same-provider continuation should retain the original `Transcript`, or use
  `save_transcript`/`restore_transcript` across processes.
- Cross-provider handoff is explicitly an export/import operation and reports
  fidelity and warnings.
- Conversion must retain user/assistant text, media references, completed tool
  calls, tool results, and stable public annotations when representable.
- It may drop provider-private reasoning state, signatures, caches, and remote
  continuation handles. Drivers must surface this; they must not silently call
  the result `Exact`.
- Unresolved tool calls are invalid input unless the conversion policy says how
  to cancel or synthesize their results.

## Tool-calling provider capability

The provider step protocol consumes the shared transcript interface:

```baml
interface ToolCallingProvider requires Provider {
  function begin<T>(self, task: Task<T>) -> Transcript
  function step<T>(self, transcript: Transcript, tools: Tool[]) -> T | ToolCalls
  function submit(
    self,
    transcript: Transcript,
    results: ToolResult[],
  ) -> Transcript
}
```

`ToolCallingProvider` does not require `GenerationProvider`. Its `step<T>`
already defines the model-turn operation needed by an agent loop, and a
provider may honestly support a tool loop without exposing a separate one-shot
generation API. Concrete OpenAI and Anthropic adapters normally implement both
capabilities; drivers depend only on the one they actually call.

An implementation must reject a transcript it does not own unless it also
implements and explicitly invokes `TranscriptImportProvider`. This avoids
accidental cross-provider mixing.

### Native tools and prompt/SAP tools

`ToolCallingProvider` describes behavior, not a required vendor wire format.
An adapter may implement a step in either of two ways:

- **Native:** send the active tool schemas through the provider's tool-calling
  request fields and decode its native tool-call response blocks.
- **Prompt/SAP:** render the step's `${ctx.output_format}` for
  `T | ToolCalls`, append the concrete JSON Schema for every active tool, ask
  the model to choose the final branch or the tool-call branch, and SAP-parse
  the result.

The prompt/SAP form must extend `ctx.output_format`; it must not require every
LLM function author to repeat a second hand-written tool protocol. This keeps
the LLM function's prompt recipe authoritative about where output instructions
appear. The extension is computed for every step from the resolved active
roster, after task tools, call-time options, registry changes, and hooks have
been applied. Adding or removing tools therefore changes the very next model
request.

Native tool transport is preferred when an adapter supports it. A provider
that lacks native tool fields may still honestly implement
`ToolCallingProvider` with the prompt/SAP strategy. Its transcript must retain
the model's tool-call value and every submitted result so the following prompt
is complete.

Application `Tool` values are dispatchable by the agent driver and may use
either strategy. Provider-owned tools are different: a web-search or
code-execution tool executed by the vendor can only be exposed when the
provider adapter also knows how it is executed and how its results re-enter
the transcript. A driver must not turn an opaque `ProviderTool` into a
model-visible prompt tool with no result path.

## `ToolCallingProvider` versus `Agent`

`ToolCallingProvider` is a low-level capability: it can begin a transcript,
perform one provider step, and accept tool results. It does not own tool
dispatch, hook ordering, budgets, provider switching, or termination policy.
Those belong to the explicit `run_agent` driver.

An application may package that driver policy as a provider whose default
`drive` runs the whole loop:

```baml
class Agent {
  inner: ToolCallingProvider,
  options: AgentOptions,

  implements Provider {}

  implements DriveProvider {
    function drive<T>(self, task: Task<T>) -> Response<T> {
      drive_agent_to_completion(self.inner, task, self.options)
    }
  }
}
```

This is why the capability is not called `AgentProvider`. A plain OpenAI or
Anthropic adapter can implement `ToolCallingProvider` without claiming to own
an agent harness. `Agent` is the concrete provider composition users select
when they want a direct `MyFunction(...)` call to run that harness by default.

## Running and observing the loop

```baml
let task = ResearchQuestion.task(q)

match (ai.drivers.run_agent(task, ai.AgentOptions {
  budget: ai.Budget { max_steps: 12 },
  hooks: MyHooks {},
})) {
  let done: ai.Done<Answer> => done.value,
  let stopped: ai.BudgetReached => queue_for_review(stopped.transcript),
  let handoff: ai.Handoff => route(handoff),
}
```

`stream_agent(task)` emits `RunStarted`, model deltas, reasoning summaries,
tool-call requested/started/finished, provider changed, tools changed, usage,
and terminal events. Hooks receive the same immutable event values. The
driver's transcript remains private mutable state.

A direct `MyFunction(...)` call uses the selected provider's `DriveProvider`.
An `Agent` drive runs its documented completion policy and either returns `T`
or throws; it never disguises `BudgetReached` or `Handoff` as model output.
Code that treats those as normal control flow uses `run_agent`.

## Step and tool-result invariants

One step is one call to `ToolCallingProvider.step`, including the final call
that returns `T`. `max_steps` is checked before starting a provider step. If a
tool round consumes the last allowed step, the driver executes and submits its
results, then returns `BudgetReached` before another provider request. This
keeps the transcript free of dangling tool calls.

Within a tool round:

1. emit the proposed call;
2. run `before_tool_call`, which may preserve, rewrite, or block it;
3. preserve the provider's call ID across every rewrite;
4. dispatch approved calls, potentially in parallel;
5. turn a blocked call into an error `ToolResult` with its original ID;
6. run `after_tool_call` only for calls that actually executed;
7. submit exactly one result for every provider-requested call ID.

Results correlate by ID, never array position. Missing or duplicate result IDs
are driver errors; a recoverable missing result may be synthesized as an error
result so the provider protocol remains complete.

## Changing provider during the loop

`prepare_step` may select a new provider:

```baml
function prepare_step(self, ctx: StepContext) -> StepPlan {
  if (ctx.usage.cost_usd > 0.25) {
    StepPlan { provider: CheapModel, tools: null, stop: null }
  } else {
    StepPlan { provider: null, tools: null, stop: null }
  }
}
```

The driver then performs explicit transcript conversion, emits a
`ProviderChanged` event including fidelity/warnings, re-renders the task for
the target provider, and continues. A target without `TranscriptImportProvider`
causes typed `Unsupported`; the driver never flattens the transcript to text
as a fallback.

The presence of `StepPlan.provider` is itself the switch command. Provider
descriptors are display data, not identity, so equal names or models do not
cancel the switch.

## Adding MCP halfway through

`StepContext` exposes a driver-owned tool registry:

```baml
function prepare_step(self, ctx: StepContext) -> StepPlan {
  if (ctx.step == 2 && !ctx.tool_registry.contains("lookup_order")) {
    let mcp = baml.mcp.connect(ctx.state.get_or_panic("mcp_url"))
    ctx.tool_registry.add_all(mcp.tools())
  }
  StepPlan { provider: null, tools: ctx.tool_registry.snapshot(), stop: null }
}
```

The next provider turn receives the new schemas. Tool execution routes through
the registry, so the same MCP connection supplies both discovery and dispatch.
Providers whose tool protocol cannot change after `begin` must say so via a
capability refinement; the safe driver rejects dynamic mutation for them.

## Background tool loops

Provider-owned tools may run inside provider background jobs. Application
tools require an application worker, so `submit_background` rejects them.
Long-lived application tool loops use `submit_harness` or a durable workflow,
where the harness/executor—not the model provider—owns scheduling and replay.

## Extensibility

Applications may define new message parts, transcript implementations,
conversion adapters, hooks, tool sources, and capability refinements through
ordinary classes/interfaces and out-of-body `implements`. The core interfaces
contain semantic minimums only; provider-specific wire fields remain in the
provider package.
