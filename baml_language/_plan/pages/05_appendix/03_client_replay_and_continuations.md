# Client replay and remote continuations

This page records an unresolved design problem in the client and journal
boundary. It is input to a redesign, not a description of settled behavior.
The current behavior remains documented in `../02_guides/05_models.md`.

The problem became visible while comparing the current BEP with pi's model
API adapters and with the OpenAI Responses and Conversations APIs. Those
systems distinguish local message replay, server-held response chains,
durable remote conversations, prompt caching, and connection affinity. The
current BEP represents most of those concepts as `raw_json` on an
`AssistantMessage` and describes all server-held state as a transport
optimization. That representation is too broad for replay and too narrow for
durable remote conversations.

## Status and scope

The following parts of the current design remain requirements:

- The journal is the durable source of truth for a normal BAML session.
- A client does not own the agent loop or execute application tools.
- A session can resume without a live client object from the earlier process.
- A session can switch clients and render canonical content for the new API.
- Streaming deltas remain ephemeral. A terminal model turn commits atomically.
- API-specific data can be retained when exact same-API replay requires
  it.

The following parts require reconsideration before the client boundary is
implemented:

- Whether `render`, `invoke`, and `ingest` should be the public client
  interface or internal phases of one model-turn operation.
- Whether a client declaration is both a service descriptor and a wire API
  adapter, or whether those concepts need separate interfaces.
- How `AssistantMessage` represents canonical text, reasoning, and tool calls.
- Which subset of a response is retained for exact replay.
- How a remote response cursor is stored, validated, invalidated, and retried.
- Whether durable remote conversations are supported at all, and which object
  owns their lifecycle.
- How remote state interacts with branching, compaction, retries, snapshots,
  and cross-client switching.

This page does not propose `begin`, `step`, or `submit` as required client
methods. Normal tool continuation can remain another model invocation whose
input contains tool results. Stateful remote resources may require optional
resource-management APIs, but they do not require every client to expose a
stateful conversation object.

## The problem

The current client contract is:

```baml
interface Client {
    function id(self) -> string
    function render(self, j: Journal, tb: Toolbox, output_schema: string) -> ProviderRequest
    function invoke(self, req: ProviderRequest) -> ProviderResponse
    function ingest(self, resp: ProviderResponse) -> Event[]
}
```

The current `AssistantMessage` stores a canonical string, a client ID, and the
complete raw response body:

```baml
class AssistantMessage {
    content: string,
    raw_json: string?,
    provider: string,
}
```

This model leaves several questions unanswered.

First, a string cannot canonically represent a response containing text,
reasoning blocks, several tool calls, per-block signatures, and API item
IDs. The reference client compensates by parsing action JSON outside the
assistant content and emitting separate action events. That makes agent output
semantics part of the OpenAI client and creates an ordering dependency between
action events and `AssistantMessage`.

Second, the complete HTTP response is not the same object as replayable model
output. An HTTP response can contain request echoes, status, usage, headers,
diagnostics, and fields that are invalid when sent as input. OpenAI stateless
replay requires the items from `response.output`, including opaque reasoning
items and assistant message phase data. Other APIs require different signed or
opaque substructures. A client therefore needs a narrow replay capsule rather
than an assumption that the complete response body can be replayed verbatim.

Third, local replay data and a remote continuation cursor have different
lifetimes. Encrypted reasoning items can be stored in the journal and replayed
years later. An OpenAI response ID normally points to a response retained for a
limited period. A connection-cached response ID may stop working as soon as a
socket closes. Treating both as one `raw_json` field hides these failure modes.

Fourth, a server-held response chain and a durable remote conversation have
different ownership semantics. A response chain is a cursor that can be
discarded when unavailable. A remote conversation is an independently stored
object that can outlive the local process and can potentially be read or
modified outside BAML. The latter is not merely a request optimization.

Fifth, the current split makes fallback orchestration unclear. A rejected
remote cursor requires the journal to be rendered again without that cursor.
Either the runner must understand a structured continuation error and call
`render` again, or the API adapter must own render, send, classification, and
fallback for one model turn.

Sixth, the current reference uses `/v1/chat/completions`. OpenAI
`previous_response_id` and `conversation` belong to `/v1/responses`. They
cannot be added as fields to the existing Chat Completions request. Supporting
them requires an OpenAI Responses adapter with a distinct capability set.

## The state classes that must remain distinct

The redesign must use separate terms for objects with different persistence
and correctness properties.

| State | Meaning | Durable locally | Required for correctness | Remote dependency |
|---|---|---:|---:|---:|
| Canonical content | BAML text, reasoning summary, tool calls, and other portable blocks | yes | yes | no |
| Replay capsule | Opaque API-native output items required for exact same-API replay | yes | sometimes | no after capture |
| Response cursor | A remote response ID used to continue from one completed response | optional | no when replay exists | yes |
| Remote conversation binding | The identity of a durable server-managed conversation object | yes, as a reference | yes in remote-conversation mode | yes |
| Prompt cache key | A hint that improves cache reuse for equivalent prompt prefixes | optional | no | usually |
| Session-affinity key | A routing hint that sends related requests to compatible infrastructure | optional | no | usually |
| Connection cache | An in-process socket and its connection-local continuation state | no | no | yes |
| Model-call attempt state | Local write-ahead evidence used to recover a call after a crash | yes at higher durability tiers | tier-dependent | no |

A prompt cache hit does not mean that the remote service owns conversation
state. A session-affinity key does not identify a conversation. A response ID
does not identify a durable Conversations API object. A replay capsule does not
grant access to the original remote response.

The journal should contain canonical content and any replay data required for
the configured durability guarantee. A remote cursor should be treated as a
checkpoint that can be ignored when it is unavailable. A remote conversation
binding should be treated as an explicit storage mode with lifecycle and
concurrency rules.

## OpenAI state mechanisms

OpenAI currently exposes three relevant context strategies through the
Responses API.

OpenAI does not define a `previous_conversation_id` field. The two remote-state
fields are `previous_response_id`, which names a parent response, and
`conversation`, which binds a response to a durable Conversations API object.

### Local replay

The caller can send the complete input explicitly on every request. For
stateless reasoning-model calls, OpenAI instructs callers to preserve every
item in `response.output`. Those items include encrypted reasoning data and
assistant message phase values that are needed for exact later replay.

This strategy works with `store: false`. It makes the local journal and replay
capsules sufficient for resume. It also allows the caller to trim, compact, or
branch context locally. The cost is a larger request body and adapter logic
that reconstructs all input items correctly.

OpenAI documents this strategy in [Manually manage conversation
state](https://developers.openai.com/api/docs/guides/conversation-state#manually-manage-conversation-state).

### Response chains

The caller can set `previous_response_id` on a new Responses request. The new
request sends only the additional input, while OpenAI supplies the context of
the referenced response chain.

The response ID is a parent cursor rather than a mutable conversation handle.
Several new responses can reference the same parent response, so this mechanism
supports forks. Each child response becomes a new branch head.

Top-level instructions from the earlier response are not carried into the new
request. The adapter must render current instructions again. Current tool and
output configuration should also be rendered as request configuration rather
than assumed to be session state.

Response objects are stored for 30 days by default. The caller can disable
normal response storage with `store: false`. An adapter must not assume that a
cursor remains usable across a process restart, retention boundary, account or
project change, or zero-data-retention configuration.

All earlier input tokens in a response chain are still billed as input tokens.
The cursor can reduce request construction and transmission without making the
earlier context free.

OpenAI documents chaining, forks, retention, and billing in [Passing context
from the previous
response](https://developers.openai.com/api/docs/guides/conversation-state#passing-context-from-the-previous-response)
and [Update multi-turn
conversations](https://developers.openai.com/api/docs/guides/migrate-to-responses#3-update-multi-turn-conversations).

### Durable remote conversations

The Conversations API creates a long-running object with a durable ID. A
Responses request can specify `conversation`. The service prepends stored
conversation items to the new input and adds the request's input and output
items to the conversation after the response completes.

Conversation items can include messages, tool calls, tool outputs, and other
data. Conversation objects and their items are not subject to the default
30-day Response TTL. A response attached to a conversation has its items
persisted with the conversation.

The API does not allow `conversation` and `previous_response_id` on the same
request. A client option with two nullable fields would admit an invalid state.

A remote conversation can outlive a BAML session process. It can also be used
from another device or job. Supporting it therefore requires decisions about
ownership, external mutation, deletion, concurrency, and synchronization. It
should not be enabled silently as an optimization.

OpenAI documents this behavior in [Using the Conversations
API](https://developers.openai.com/api/docs/guides/conversation-state#using-the-conversations-api)
and the [Create a response API
reference](https://developers.openai.com/api/reference/resources/responses/methods/create).

### WebSocket continuation

The OpenAI Responses WebSocket transport uses the same
`previous_response_id` field. A connection-local cache can retain recent
response state for low-latency continuation. If the ID is no longer available,
the caller can set the cursor to `null` and send full context.

Connection-local continuation has a shorter and less durable lifetime than a
normally stored Response object. A client should classify it as an optional
transport optimization even when the cursor syntax is identical.

OpenAI documents this behavior in [`previous_response_id` in WebSocket
mode](https://developers.openai.com/api/docs/guides/conversation-state#previous_response_id-in-websocket-mode).

## What pi currently does

This section describes pi at commit
[`6b461b75`](https://github.com/earendil-works/pi/tree/6b461b75b39b5a19b378dc42fbfbd1655bc446a6),
reviewed on 2026-08-05. The commit was the repository HEAD at the time of the
review. The behavior is implementation evidence rather than a requirement for
BAML.

### The normal OpenAI Responses adapter

The normal `openai-responses` adapter constructs complete `input` from pi's
local context and sets `store: false`. Its dedicated OpenAI options add
reasoning effort, reasoning summary, service tier, and tool choice. The options
inherit common controls such as `sessionId`, `samplingParams`, `onPayload`,
retry limits, and transport selection. There is no dedicated
`previous_response_id` or OpenAI conversation option.

The relevant source is [OpenAI Responses options and request
construction](https://github.com/earendil-works/pi/blob/6b461b75b39b5a19b378dc42fbfbd1655bc446a6/packages/ai/src/api/openai-responses.ts#L90-L96)
and [`buildParams`](https://github.com/earendil-works/pi/blob/6b461b75b39b5a19b378dc42fbfbd1655bc446a6/packages/ai/src/api/openai-responses.ts#L260-L335).

Pi retains API-native information inside its local assistant content:

- `AssistantMessage.responseId` stores the upstream response or message ID.
- A reasoning block's `thinkingSignature` stores the serialized reasoning item,
  including encrypted content when available.
- A text block's `textSignature` stores the output message ID and phase.
- An OpenAI tool-call ID combines the function `call_id` and the response item
  ID so both can be reconstructed.
- Conversion code drops or normalizes IDs that are invalid after a model or
  API or service change.

The source is [pi assistant message
types](https://github.com/earendil-works/pi/blob/6b461b75b39b5a19b378dc42fbfbd1655bc446a6/packages/ai/src/types.ts#L340-L364),
[`AssistantMessage.responseId`](https://github.com/earendil-works/pi/blob/6b461b75b39b5a19b378dc42fbfbd1655bc446a6/packages/ai/src/types.ts#L412-L426),
and [OpenAI Responses message
conversion](https://github.com/earendil-works/pi/blob/6b461b75b39b5a19b378dc42fbfbd1655bc446a6/packages/ai/src/api/openai-responses-shared.ts#L130-L290).

This representation gives pi stateless resume and local branching without a
remote response chain. It also gives the adapter enough information to retain
same-API reasoning and tool-call fidelity. Conversion to another API uses the
portable content and discards or normalizes incompatible signatures.

### Raw payload escape hatches

Pi applies `samplingParams` after its named request fields. A caller can
therefore override `store` and inject `previous_response_id`. Pi also invokes
`onPayload` after constructing the request and permits the callback to replace
the payload.

Injecting only `previous_response_id` is not a correct continuation
implementation. The normal adapter has already placed the complete local
context in `input`, so the earlier context can be supplied once through the
response chain and again through the full input. A safe external implementation
must also replace `input` with the delta and manage cursor compatibility,
failure, and update.

These hooks prove that continuation can be layered over a request adapter. They
do not provide lifecycle-safe continuation as a normal pi feature. The relevant
source is [the final request merge and payload
callback](https://github.com/earendil-works/pi/blob/6b461b75b39b5a19b378dc42fbfbd1655bc446a6/packages/ai/src/api/openai-responses.ts#L128-L150)
and [`samplingParams` merge
order](https://github.com/earendil-works/pi/blob/6b461b75b39b5a19b378dc42fbfbd1655bc446a6/packages/ai/src/api/openai-responses.ts#L330-L333).

### The Codex WebSocket adapter

The `openai-codex-responses` adapter has a narrow connection-cached
continuation path. With `transport: "websocket-cached"` or `"auto"`, pi stores
the last request body, response ID, and response items on a cached WebSocket
connection.

Before reusing the response ID, pi compares the non-input request body with the
previous request. It then verifies that the new full input begins with the old
input followed by the earlier response items. When both checks pass, it sends
only the remaining input and sets `previous_response_id`.

When the comparison fails, pi clears the cached continuation and sends the full
input. When the remote service reports that the previous response is missing,
pi clears the connection continuation and retries. A transport failure before
streaming can fall back to the full-context SSE path. A new process or lost
connection also loses this continuation state.

The relevant source is [cached input-delta
selection](https://github.com/earendil-works/pi/blob/6b461b75b39b5a19b378dc42fbfbd1655bc446a6/packages/ai/src/api/openai-codex-responses.ts#L1387-L1438),
[WebSocket request and checkpoint
update](https://github.com/earendil-works/pi/blob/6b461b75b39b5a19b378dc42fbfbd1655bc446a6/packages/ai/src/api/openai-codex-responses.ts#L1455-L1538),
and [missing-cursor retry and transport
fallback](https://github.com/earendil-works/pi/blob/6b461b75b39b5a19b378dc42fbfbd1655bc446a6/packages/ai/src/api/openai-codex-responses.ts#L307-L380).

Pi's `sessionId` helps key this in-process connection cache. In the normal
OpenAI Responses adapter it is used for prompt-cache and session-affinity
fields. It is not an OpenAI conversation ID in either case.

### OpenAI Conversations support

No OpenAI Conversations API integration is present in the reviewed pi commit.
Pi contains other APIs whose names include conversations, such as its Mistral
adapter, but those do not establish OpenAI Conversations behavior.

## Lessons from pi

Pi provides evidence for several design choices.

The local representation should be sufficient for resume. Server state can
then be used opportunistically without making serialized sessions depend on
the remote retention period.

Provider-specific replay data does not need to be the complete HTTP response.
Pi stores IDs and opaque signatures at the content blocks that require them.
BAML can instead use an opaque replay capsule per assistant turn, but the
capsule should contain only data that the API adapter can replay.

A continuation decision belongs to the API adapter. The Codex adapter knows
which request fields must match, how to compare the old context with the new
context, and which errors mean that a cursor can be discarded. The generic
runner should not contain OpenAI request-shape logic.

A continuation checkpoint does not make the client stateful. Pi's normal
context remains an input value. The cached adapter reads a checkpoint, performs
one call, and produces a new checkpoint. BAML can persist the checkpoint on the
assistant journal entry instead of mutating the client object.

Fallback must preserve the full local input path. Pi can always send the
already-constructed complete input when its cached continuation is not usable.
BAML must retain the same property after compaction, snapshots, and client
switches.

`sessionId` is too overloaded to model conversation state. A BAML session ID,
prompt cache key, connection-cache key, remote response ID, and remote
conversation ID need separate fields and documented lifetimes.

## Relationship to BEPv4

The earlier BEPv4 design in the `aaron/custom-llm-providers-v4` checkout uses
an explicit provider-owned `Conversation` for every normal model run. Its
`AgentProvider` protocol is:

```baml
interface AgentProvider requires Provider {
    function begin<T>(self, task: Task<T>) -> Conversation
    function step<T>(
        self,
        conversation: Conversation,
        tools: ai.tools.Tool[],
    ) -> ModelStep<T>
    function submit(
        self,
        conversation: Conversation,
        results: ai.tools.ToolResult[],
    ) -> Conversation
}
```

`begin` creates local provider-owned conversation state without a model
request. `step` performs one model request. `submit` validates correlated
application-tool results and mutates the conversation without performing the
next model request. The Agent runner owns the loop and tool execution.

The BEPv4 OpenAI Responses implementation stores the rendered input,
`previous_response_id`, active tools, last output items, portable message
history, request URL, and non-authentication headers in its concrete
conversation. Its normal step sets `store: true`, sends the current input, and
adds `previous_response_id` after the first response. This proves that the
begin/step/submit protocol can implement response chaining, but it makes a
mutable client-owned conversation the primary state model.

Several BEPv4 concepts should remain in the redesign:

- `Task<T>` reifies typed model work independently of a particular run.
- `ModelStep<T>` separates one model request from a complete agent run.
- The Agent, rather than the client, owns the model/tool loop.
- Application tool results retain exact call correlation.
- Failure classification distinguishes replay-safe rejection from ambiguous
  remote failure.
- A failed model step cannot commit a partial local continuation update.
- Provider wrappers preserve ownership identity through explicit delegation.
- Conversation import reports fidelity when converting portable content to a
  destination API.
- `ProviderDataPart` recognizes that some message data is API-specific and
  must not be treated as portable text.
- Realtime, streaming, background, and batch execution remain optional
  capabilities rather than obligations of the normal model API.

Several BEPv4 choices now require reconsideration:

| BEPv4 choice | Reconsideration |
|---|---|
| Every run calls `begin` | A stateless API adapter can render the first request directly from a task or journal. It does not need an empty mutable conversation object before the first request. |
| Every turn calls `step(conversation, tools)` | One `invoke(ModelTurnInput)` operation can read a journaled checkpoint and return a terminal turn plus a new checkpoint. The configured client remains reusable across sessions. |
| Tool results go through `submit` | The runner can append correlated tool-result events. The next ordinary invocation lowers them as new input. Validation remains necessary, but it does not need to be a client lifecycle method. |
| `Conversation` owns exact continuation state | Canonical content and replay capsules belong in the journal. A remote response cursor can be an optional checkpoint on the assistant entry. |
| `ConversationAppendProvider` mutates in place | Journal append is a better durability boundary for application messages. In-place mutation complicates snapshots, branches, concurrent readers, and recovery. |
| Provider-instance identity owns conversations | A wire-domain compatibility value can validate replay and cursor scope without requiring a live object identity after snapshot and resume. |
| `save_conversation` and `restore_conversation` are client capabilities | Normal session persistence should use the journal. Remote resources can expose separate optional binding or resource-management capabilities. |
| OpenAI response chaining is the normal path | Local replay should remain complete. Response chaining can be selected when storage, retention, ancestry, and compatibility conditions permit it. |

`submit` remains a useful conceptual boundary for validation. Every pending
tool call must receive one correlated success or error before another model
step is rendered. The redesign can enforce that invariant in the runner and
journal fold rather than in every API adapter.

`begin` can remain meaningful for a genuinely stateful resource such as a
realtime session or an explicitly created remote conversation. That lifecycle
should be an optional capability with resource semantics. It should not shape
the minimum interface for Chat Completions, Responses local replay, Anthropic
Messages, or another request/response model API.

The BEPv4 transactional invariant also needs a remote qualification. A client
can promise that a failed call does not advance local state. It cannot always
promise that the remote service saw no request or performed no hosted-tool
effect. Retry safety must therefore depend on classified failure timing rather
than only on whether the local `Conversation` object was mutated.

The redesign should retain BEPv4's typed task, outcomes, call correlation,
classified failures, capability interfaces, and Agent ownership boundary. It
should replace the mandatory provider-owned conversation lifecycle with
journaled canonical state, narrow replay data, and optional remote-state
capabilities.

## Candidate local data model

The following types illustrate the information that the redesign needs. They
are not accepted BAML API names.

```baml
class WireDomain {
    service_id: string, // "openai"
    api_id: string,     // "responses"

    // Adapter-generated compatibility scope. It can account for base URL,
    // account or project, auth scope, and protocol version. It contains no
    // credential or other secret.
    scope: string,
}

class ReplayCapsule {
    domain: WireDomain,
    items_json: string,
}

class RemoteContinuation {
    domain: WireDomain,
    kind: string,
    state_json: string,
}

class CanonicalAssistant {
    content: ModelContent[],
    service_id: string,
    api_id: string,
    model_id: string,
    replay: ReplayCapsule?,
    continuation: RemoteContinuation?,
}
```

`WireDomain` is narrower than a BAML session and broader than an individual
response. The API adapter creates and compares the value. The runner treats it
as opaque compatibility metadata.

`ReplayCapsule` is durable local data. For OpenAI Responses it can contain the
replayable `response.output` items. It should not contain the API key, response
headers, arbitrary transport diagnostics, or the entire HTTP envelope unless
an independent trace feature elects to retain those fields.

`RemoteContinuation` is a tagged opaque value. A generic class with
`response_id: string?` and `conversation_id: string?` is not sufficient because
it admits both fields or neither field. The API adapter should decode the state
only when `domain` and `kind` match.

For OpenAI, the adapter-private state might be:

```baml
class OpenAiResponseChainState {
    previous_response_id: string,
}

class OpenAiConversationState {
    conversation_id: string,
}
```

The assistant journal entry is the natural checkpoint for a response cursor.
Its sequence number identifies the point after which new promptable entries
must be lowered as a delta. A separate `covered_through` field is unnecessary
when the continuation is stored on that entry.

Durable remote conversations may need a different journal representation. A
candidate design records a binding event when the remote object is created or
attached and a checkpoint after each synchronized turn:

```baml
class RemoteConversationBound {
    domain: WireDomain,
    kind: string,
    state_json: string,
}

class RemoteConversationAdvanced {
    domain: WireDomain,
    through_seq: int,
    state_json: string,
}
```

This representation makes the remote object lifecycle visible in the journal.
It also separates a persistent binding from a parent response cursor attached
to one assistant turn. The redesign must decide whether this distinction is a
public feature or an internal event detail.

## Candidate client boundary

The user-visible `client<llm>` declaration can remain the model, endpoint,
authentication, and retry configuration selected by an LLM function. The
runtime underneath it may need two layers:

- A service descriptor resolves authentication, base URL, model metadata, and
  compatibility flags.
- An API adapter implements one wire protocol such as OpenAI Chat Completions,
  OpenAI Responses, Anthropic Messages, or Mistral Conversations.

An OpenAI-compatible service can then reuse an existing API adapter. A service
that exposes both Chat Completions and Responses can bind models to different
adapters. Continuation capabilities belong to the adapter binding rather than
the service name alone.

The public runtime interface can be one model-turn operation:

```baml
class ModelTurnInput<E> {
    journal: Journal<E>,
    toolbox: Toolbox,
    instructions: string,
    output_schema: string?,
    context_policy: ContextPolicy,
}

class ModelTurn {
    assistant: CanonicalAssistant,
    usage: Usage?,
    stop_reason: StopReason,
    response_id: string?,
}

interface ModelApi {
    function id(self) -> string
    function capabilities(self) -> ModelApiCapabilities
    function invoke(self, input: ModelTurnInput) -> ModelTurn throws unknown
}
```

Rendering, transport, and ingestion still exist. They can remain internal pure
or independently testable functions. Making `invoke(ModelTurnInput)` the public
operation lets the adapter rerender after a rejected cursor without requiring
the runner to understand wire request bodies.

The alternative is to retain the current public three-phase interface and add
a structured `ContinuationRejected` error. The runner would catch that error,
call `render` with continuations disabled, and invoke again. This keeps
rendering visible but moves continuation policy and retry safety into the
runner. The redesign should compare these options explicitly.

The runner should derive journal events from the terminal `ModelTurn`. It can
atomically commit `AssistantMessage`, `ToolRequested`, usage, replay data, and
the new continuation. The API adapter should not execute application tools,
produce `FinalProduced`, or implement the generated output parser.

## Context policies and capabilities

Remote state should be selected by an explicit policy. A candidate policy is:

```baml
enum ContextPolicy {
    // Render all context from the local journal and replay capsules.
    Local

    // Use a compatible response cursor when available. Fall back to Local.
    PreferResponseChain

    // Bind the session to a durable remote conversation object.
    RemoteConversation
}
```

`Local` supports `store: false`, local branching, deterministic compaction
instructions, and cross-client resume.

`PreferResponseChain` preserves local durability while allowing an adapter to
send a delta. It must define which errors permit an automatic local fallback.

`RemoteConversation` opts into remote persistence. It must define binding,
ownership, synchronization, concurrency, deletion, privacy, and fallback
behavior. A failure to access the remote conversation may be terminal rather
than an automatic local replay because the remote object might contain items
that are not present locally.

Capabilities need to be attached to an API adapter and model binding. A
candidate shape is:

```baml
enum ContinuationCapability {
    ResponseChain
    ConnectionCachedResponseChain
    RemoteConversation
}

class ModelApiCapabilities {
    streaming: bool,
    tools: bool,
    structured_output: bool,
    continuations: ContinuationCapability[],
}
```

Prompt caching and session affinity should remain separate capabilities or
options. They do not change which journal entries are lowered into model input.

OpenAI-compatible endpoints require compatibility flags. An endpoint can
implement Chat Completions without Responses, Responses without Conversations,
or a partial Responses dialect that ignores remote-state fields. The service
descriptor or model catalog must not infer continuation support only from an
OpenAI-compatible base URL.

## Response-chain rendering algorithm

The following algorithm preserves the local journal as the source of truth.

1. The API adapter computes the current `WireDomain`.
2. It searches the current branch ancestry for the newest assistant entry with
   a compatible response-chain continuation.
3. It verifies that no event after the checkpoint invalidates the remote
   representation.
4. It lowers only new model input after the checkpoint.
5. It renders current top-level instructions, tools, output constraints, and
   other request configuration.
6. It invokes the remote API with the parent response cursor and the delta.
7. It ingests the terminal response into canonical content, a replay capsule,
   usage, stop reason, and a new cursor.
8. The runner atomically appends all events derived from that terminal turn.

If no compatible checkpoint exists, the adapter lowers the complete effective
journal. Effective rendering applies compaction instructions and skips
journal-only events. Same-domain assistant entries use replay capsules when
required. Foreign-domain entries lower from canonical blocks.

A cursor can be ignored when the domain differs, the remote response is
missing, retention has expired, the configured policy is local-only, or a
local rendering change makes the remote prefix invalid.

Fallback should occur only for a classified continuation failure before any
model output or remote side effect has been accepted. Retrying after partial
output can duplicate token cost. Retrying a response that invoked a hosted
tool can duplicate external effects. A generic network error is not sufficient
evidence that a full-context retry is safe.

## Tool-loop continuation

A tool result does not require a client `submit` method. It is new input to the
next model invocation.

Assume a response produces an OpenAI function call and the runner commits:

```text
seq 9   AssistantMessage
          content = ToolCall(call_7, search, ...)
          continuation = resp_123
seq 10  ToolRequested(call_7, search, ...)
seq 11  ToolCompleted(call_7, {"results":[...]})
```

The `ToolRequested` event is a control and audit projection of the structured
tool call already present in `AssistantMessage`. It must not become a duplicate
model input item. The OpenAI Responses adapter lowers only the tool result:

```json
{
  "model": "gpt-5.6",
  "previous_response_id": "resp_123",
  "input": [
    {
      "type": "function_call_output",
      "call_id": "call_7",
      "output": "{\"results\":[...]}"
    }
  ]
}
```

This algorithm requires structured assistant content. A string-only
`AssistantMessage` cannot determine that `ToolRequested` mirrors an existing
API output item. The current action-parser arrangement should therefore
be reconsidered together with continuation support.

Parallel tool calls require all function-call IDs and API item IDs to be
preserved. Results can arrive in a different order from requests. The adapter
must correlate each result by call ID and lower the set of completed results
per API rules.

## Branching, rewinds, and compaction

A response cursor is valid for any branch whose ancestry contains its
checkpoint and whose new input is a valid delta from that checkpoint. A branch
created after `resp_123` can reuse `resp_123` as its parent. Two branches then
receive different child response IDs.

The rule is ancestry, not the absence of branching:

```text
                         branch A input -> resp_A
assistant(resp_123) ----+
                         branch B input -> resp_B
```

A rewind or edit before the checkpoint invalidates that checkpoint for the new
branch. The renderer must search for an earlier compatible checkpoint or use
full local replay.

Local compaction usually invalidates a remote response chain. The remote chain
still contains the original prefix, while the local effective transcript now
contains a summary in its place. Continuing from the old response would bypass
the local compaction decision. The adapter should start a new chain by rendering
the compacted effective journal.

A remote service's server-side compaction can be reused only if the journal records
the resulting semantic checkpoint and local rendering can reproduce or safely
reference it. Server-side compaction should not be assumed equivalent to the
existing `Compacted { summary, through_seq }` event.

A durable remote conversation has different branch semantics. Appending two
branches to one remote conversation mixes their items into one object. A true
fork therefore requires a new remote conversation seeded from local replay, or
the product must prohibit branching in remote-conversation mode.

## Client switching and replay domains

The continuation domain must include more than the display client ID. The same
service can expose several APIs. The same API can be reached through different
base URLs, projects, accounts, gateways, and credentials. A response ID from
one scope may be inaccessible or semantically invalid in another scope.

The adapter computes a scope that is stable enough to compare and contains no
secret. The exact construction remains an open design question. It may include
resolved endpoint identity, account or project identity when available, API
version, and relevant compatibility flags.

Changing to a foreign domain discards the remote cursor. The new adapter lowers
canonical content. It does not send another API's encrypted reasoning item or
signed metadata.

Changing to the same domain can preserve replay fidelity even when remote
continuation is disabled. A replay capsule and a cursor therefore need separate
compatibility checks. Some model changes within one API can reuse canonical
content but not old API item IDs. The adapter, rather than the runner,
must decide that compatibility.

## Durability and crash recovery

The terminal assistant content, usage, replay capsule, and new continuation
must commit in one journal batch. A committed assistant entry must never lack
the replay information required by the configured local durability mode.

A crash can occur after the remote service completed a response but before the
local batch committed. The remote response then exists without a local
checkpoint. A retry can create a second response and duplicate token cost. The
journal remains internally correct because it never advanced to an uncommitted
cursor, but higher durability tiers may need model-call attempt records to
reconcile or diagnose the orphan.

A remote conversation adds another crash window. Creating the remote object is
an external side effect. A crash between creation and appending
`RemoteConversationBound` leaves an orphan object. Appending a planned binding
before the remote object exists leaves a local record that needs settlement.
The remote-conversation design must specify a write-ahead or reconciliation
protocol before claiming tier-2 durability.

Automatic fallback after an invalid cursor is a second remote model attempt. It
must share the original turn's retry budget and observability record. It must
not be confused with an application-level retry of the entire agent turn.

Hosted API tools complicate retries because the remote service can perform
effects during response generation. The durability model in
`../02_guides/12_durability.md` currently focuses on BAML tools. The redesign
must state whether hosted tools are replay-safe, externally idempotent, or
excluded from automatic fallback after ambiguous failures.

## Privacy, retention, and billing

Local replay and remote continuation have different data policies.

OpenAI Responses are stored for 30 days by default. `store: false` disables
normal response storage. OpenAI Conversations persist their items without the
30-day Response TTL. A BAML option that silently changes from local replay to a
remote chain or conversation can therefore change data retention.

The context policy should be explicit in traces and snapshots. An application
should be able to select local-only behavior for zero-data-retention or other
privacy requirements. A remote-conversation mode should document deletion and
retention responsibilities.

Replay capsules can also contain sensitive opaque data. Encrypted reasoning
content is opaque to the application but still belongs to a user's model
interaction. Snapshot encryption, redaction, export, and telemetry policies
must account for replay capsules separately from visible canonical text.

`previous_response_id` does not remove earlier tokens from billing. Cost
accounting should use API-reported usage rather than estimating savings
from the smaller transmitted delta. Prompt-cache usage is a separate usage
dimension and should not be represented as continuation success.

## Scenarios the redesign must specify

The final design and reference implementation should cover the following
scenarios with executable tests.

1. A local-only OpenAI Responses session persists replayable output items with
   `store: false`, snapshots, resumes in a new process, and continues.
2. A response-chain session sends full input on the first call and only new
   input on the second call.
3. A response-chain tool loop sends a `function_call_output` without duplicating
   the earlier function-call item.
4. Two branches reuse one parent response ID and receive separate child IDs.
5. A rewind before a checkpoint prevents that checkpoint from being selected.
6. Local compaction starts a new remote chain whose first request contains the
   compacted effective transcript.
7. Switching to a foreign client lowers canonical content and drops the remote
   cursor and incompatible replay items.
8. Switching back to a compatible client uses retained local replay even when
   the original remote response has expired.
9. A missing response ID triggers at most one classified full-context fallback
   before any output is emitted.
10. A network failure after partial output does not automatically issue a
    second model call.
11. A session configured with `store: false` does not claim durable remote
    continuation.
12. A prompt-cache key or BAML session ID is never interpreted as a remote
    conversation ID.
13. A remote conversation survives process restart and records its binding and
    synchronized journal position.
14. Two concurrent calls against one remote conversation are serialized or
    rejected according to documented semantics.
15. A remote conversation cannot be forked silently into the same remote
    object.
16. A crash after remote response completion but before local commit produces a
    diagnosable orphan attempt and no advanced local cursor.
17. Reasoning items, assistant phase, message IDs, function-call IDs, and item
    IDs survive same-domain replay.
18. Foreign-domain rendering never forwards signed or encrypted API data
    that the destination API cannot accept.
19. An OpenAI-compatible Chat Completions endpoint reports no response-chain
    capability even when its service name is `openai`.
20. An OpenAI Responses endpoint that lacks Conversations support can still use
    local replay and, when supported, response chaining.

## Alternatives to reconsider

The redesign should evaluate the following alternatives explicitly.

### Complete raw response versus narrow replay capsule

The current design stores the complete response body. A narrow capsule reduces
journal size, avoids retaining irrelevant transport fields, and gives the API
adapter an explicit replay contract. A complete response can still be retained
by observability infrastructure, but audit storage and model-input replay should
not be the same field by default.

### String assistant content versus canonical content blocks

A string is simple but cannot represent structured tool calls or replayable
reasoning. Canonical blocks such as `Text`, `Thinking`, and `ToolCall` allow the
runner to derive audit events without asking the client to parse the agent's
control protocol. This change is a prerequisite for correct tool-result delta
rendering.

### Three public phases versus one public model-turn operation

Public `render`, `invoke`, and `ingest` functions make each phase directly
testable. They also expose orchestration details to the runner and complicate a
safe fallback that requires rerendering. One public `invoke(ModelTurnInput)`
keeps the client simple while allowing internal pure helpers. Tests can target
those helpers without making them the stable interface.

### One client abstraction versus service descriptor plus API adapter

The current client combines endpoint, authentication, model, retry policy,
rendering, and transport. Pi separates its provider metadata from reusable API
implementations. A similar split can make an OpenAI-compatible service mostly
configuration while keeping actual protocol code in one adapter.

The split must not add stateful conversation methods to ordinary service
descriptors. It should clarify capability ownership and code reuse.

### Mutable client conversation versus journaled checkpoint

A mutable client object is easy inside one process but fails snapshots,
branching, and concurrent sessions that share a configured client. A checkpoint
stored on the assistant entry preserves those properties and keeps the client
reusable.

### One generic continuation versus separate response-chain and remote-thread concepts

One opaque tagged type keeps the runner generic. Separate concepts express the
different lifetime, ownership, and concurrency rules more accurately. The
current leaning is to store response cursors on assistant entries and model
durable remote conversation bindings as explicit journal state. The public API
shape remains unresolved.

### Automatic remote state versus explicit context policy

Automatic response chaining can change retention and fallback behavior.
Automatic remote conversations create even larger semantic changes. An explicit
context policy is easier to reason about, test, trace, and document. A future
`Auto` mode can select only among strategies whose retention and durability
semantics are declared equivalent by configuration.

### No server state versus optional server state

Always using local replay has the smallest state model and strongest local
portability. Optional response chaining can reduce request payload size and
preserve server-side reasoning or connection optimizations. The local replay
path should be implemented first so the optimization never becomes the only
recovery path.

### Branch prohibition versus ancestry-based cursor selection

Prohibiting every branch is unnecessarily restrictive for parent response
cursors. Ancestry-based selection allows valid forks and invalidates only
branches that diverge before the checkpoint. Durable remote conversations still
need a stricter branch rule.

### Remote conversation as optimization versus storage mode

Treating a remote conversation as an optimization hides its independent
persistence and mutation surface. The redesign should classify it as a storage
mode or omit it from the first implementation. It should not share default
semantics with `PreferResponseChain`.

## Open questions

The redesign must answer these questions before the appendix can be converted
into settled guide behavior.

1. Is the stable public interface `invoke(ModelTurnInput)`, or do
   `render`/`invoke`/`ingest` remain public?
2. Does BAML expose a service-descriptor and API-adapter split, or keep that
   split internal to client resolution?
3. Which canonical `ModelContent` variants are required for text, reasoning,
   tool calls, media, citations, hosted tools, and refusal data?
4. Is replay stored once per assistant turn, once per content block, or in both
   forms depending on the API?
5. How is `WireDomain.scope` calculated without persisting credentials?
6. Which model or option changes remain compatible with one response chain?
7. Which structured errors prove that a cursor failed before generation and
   permit automatic fallback?
8. How are cursor fallback attempts represented in traces and retry budgets?
9. Does local `Compacted` always break a remote chain?
10. Can service-side compaction produce a journaled checkpoint with equivalent
    semantics?
11. Is `ContextPolicy.Local` the default for portability and privacy, or is
    response chaining preferred when the API supports it?
12. Does the API expose a `RequireResponseChain` mode whose failure is terminal?
13. Are remote conversations in scope for the first implementation?
14. If remote conversations are supported, which component creates, attaches,
    lists, retrieves, and deletes them?
15. Can a remote conversation contain items added outside BAML, and how are
    those items synchronized into the journal?
16. Does a remote conversation use a dedicated journal store, a client option,
    or built-in binding and checkpoint events?
17. How are concurrent turns on one remote conversation serialized?
18. What are the maximum journal size and redaction rules for replay capsules?
19. Are raw API responses retained separately for observability?
20. How do background Responses, hosted tools, and deferred remote work fit
    the continuation and durability model?
21. Which continuation capabilities are static model metadata, configured
    compatibility flags, or dynamically probed behavior?
22. Should response IDs and remote conversation IDs be exposed to applications,
    or remain internal journal metadata?

## Recommended redesign sequence

The implementation should proceed in an order that preserves a working local
fallback at every stage.

1. Replace string-only assistant content with canonical content blocks.
2. Define a narrow replay-capsule contract and remove replay dependence on the
   complete HTTP response envelope.
3. Move generated output parsing and `FinalProduced` decisions out of the
   OpenAI adapter. Make the runner derive built-in events from a terminal model
   turn.
4. Decide the public client boundary and the service-descriptor/API-adapter
   split.
5. Implement and test local-only rendering for Chat Completions, OpenAI
   Responses, and one non-OpenAI API.
6. Add opaque response-chain checkpoints as an opt-in OpenAI Responses
   capability.
7. Implement ancestry checks, tool-result deltas, classified fallback,
   compaction invalidation, and cross-client switching tests.
8. Integrate continuation attempts with retry budgets, streaming safety,
   observability, and tier-2 durability records.
9. Decide whether durable remote conversations are a BAML v1 feature. If they
   are, design their lifecycle and journal binding separately before adding the
   OpenAI API calls.
10. Rewrite `../02_guides/05_models.md` as settled behavior and update the
    reference implementation only after these decisions are resolved.

The first implementation should not make OpenAI Conversations a hidden default.
It should not depend on remote response retention for session resume. It should
not add mandatory `begin`, `step`, or `submit` methods to every client.

## Source index

Primary OpenAI sources:

- [Conversation state](https://developers.openai.com/api/docs/guides/conversation-state)
- [Create a response](https://developers.openai.com/api/reference/resources/responses/methods/create)
- [Migrate to the Responses API](https://developers.openai.com/api/docs/guides/migrate-to-responses)
- [Reasoning continuity](https://developers.openai.com/api/docs/guides/reasoning#preserve-reasoning-across-calls)
- [Function calling](https://developers.openai.com/api/docs/guides/function-calling)
- [WebSocket mode](https://developers.openai.com/api/docs/guides/websocket-mode)

Primary pi sources at the reviewed commit:

- [Repository tree](https://github.com/earendil-works/pi/tree/6b461b75b39b5a19b378dc42fbfbd1655bc446a6)
- [Normal OpenAI Responses adapter](https://github.com/earendil-works/pi/blob/6b461b75b39b5a19b378dc42fbfbd1655bc446a6/packages/ai/src/api/openai-responses.ts)
- [OpenAI Responses conversion and replay](https://github.com/earendil-works/pi/blob/6b461b75b39b5a19b378dc42fbfbd1655bc446a6/packages/ai/src/api/openai-responses-shared.ts)
- [Codex Responses adapter](https://github.com/earendil-works/pi/blob/6b461b75b39b5a19b378dc42fbfbd1655bc446a6/packages/ai/src/api/openai-codex-responses.ts)
- [Core message and stream types](https://github.com/earendil-works/pi/blob/6b461b75b39b5a19b378dc42fbfbd1655bc446a6/packages/ai/src/types.ts)

BEPv4 sources in the comparison branch:

- [BEPv4 proposal](https://github.com/boundaryml/baml/blob/aaron/custom-llm-providers-v4/baml_language/_plan/bepv4/README.md)
- [Provider and AgentProvider protocol](https://github.com/boundaryml/baml/blob/aaron/custom-llm-providers-v4/baml_language/crates/baml_builtins2/baml_std/ai/provider/protocol.baml)
- [OpenAI Responses conversation state](https://github.com/boundaryml/baml/blob/aaron/custom-llm-providers-v4/baml_language/crates/baml_tests/baml_src_temp2/ns_openai/ns_internal/responses/conversation.baml)
- [OpenAI Responses begin and step implementation](https://github.com/boundaryml/baml/blob/aaron/custom-llm-providers-v4/baml_language/crates/baml_tests/baml_src_temp2/ns_openai/responses/tool_calling.baml)

The BEPv4 comparison used the local
`aaron/custom-llm-providers-v4` checkout at commit
`89be8b7f2095778e8ec3e232295a52712889f53a`. The linked remote branch can lag
that local commit.
