# Future phases

This page records what is deliberately absent from this BEP and how
each absence resolves. Nothing here is settled design; it exists to
show that the phase 1 surface does not have to change to admit it.

## The two invariants

Every later phase is additive because phase 1 holds two rules:

1. Assistant content is structured blocks. Tool results correlate by
   id, foreign turns re-render from canonical content, and delta
   rendering (phase 3) can distinguish "already sent" from "new
   input".
2. The journal alone renders a complete request. Server-held state is
   an optimization with a discard path, never the only copy of a
   conversation.

The extension mechanics are equally fixed: new optional fields on
existing classes (`ModelTurn`, `AssistantMessage`, `ModelTurnInput`),
new optional interfaces discovered by `match`, new journal event
types, and new content block kinds behind the existing union. No phase
removes or repurposes a phase 1 name.

## Phase 2 — fidelity and streaming

**Replay capsules.** Same-provider fidelity needs API-native data that
canonical blocks deliberately omit: signed reasoning items, output
item ids, phase markers. A capsule is an opaque per-turn value tagged
with a wire domain (service, API, compatibility scope), stored as an
optional field on the assistant entry. The rendering rule: same
domain replays the capsule, foreign domain lowers canonical blocks and
drops signed data. The capsule is narrow by contract — replayable
output items, never the HTTP envelope.

**Native structured output.** `Native` and `Strict` join `Sap` as
values of the built-in clients' `output_mode` field: OpenAI strict
schemas, Anthropic `output_config`, Gemini `responseJsonSchema` with
the reserved-tool fallback when tools are present. The interface
already carries the output type as a runtime value, so these are
field values, not interface changes.

**Streaming.** A `StreamingClient` interface adds an invoke variant
that feeds deltas to an ephemeral sink and returns the same terminal
`ModelTurn`. The journal records final turns only, so a streamed run
and a blocking run produce identical journals.

**The `PromptTools` wrapper.** One wrapper client provides prompt-mode
tool calling for models with unreliable native tool support and for
wire APIs without function calling: it renders the tool catalog and a
calls protocol into the instructions, passes the inner client an empty
toolbox, and rewrites a recognized calls envelope into `ToolUse`
blocks (`../02_guides/03_clients/05_the_built_in_clients.md`).

**Media outputs.** When the return type is exactly `image` or
`image[]`, the final value is the terminal turn's `Media` blocks
rather than parsed text. A media type nested inside a structured
output stays rejected at spec creation, because no wire protocol binds
image data into a JSON field. Image-producing clients (Gemini image
models) normalize inline parts to `Media` blocks; OpenAI image
generation is a hosted tool and follows hosted-tool support in phase
4.

## Phase 3 — continuations

Wire APIs with server-held response chains (OpenAI Responses
`previous_response_id`) can send only the delta since a checkpoint. The
design, from the sessions draft's analysis:

- A continuation checkpoint is an optional, domain-tagged, opaque
  value on the assistant journal entry. It is never a mutable client
  object.
- A `context_policy` field on `ModelTurnInput` defaults to local
  rendering; response chaining is opt-in.
- The client selects the newest compatible checkpoint from the
  journal, lowers only events after it — a tool result becomes a lone
  result item against the cursor — and re-renders instructions fresh,
  because chains do not carry them.
- A rejected cursor falls back to full local rendering, at most once,
  only for failures classified as pre-generation. This fallback is why
  `invoke` owns rendering.
- Local compaction or a rewind before the checkpoint invalidates it;
  ancestry decides reuse.

## Phase 4 — remote state and long-running work

Durable remote conversations (the OpenAI Conversations API) are a
storage mode, not an optimization: the remote object can outlive the
process and hold items with no local copy. Supporting them requires
explicit binding events in the journal, lifecycle and concurrency
rules, and a decision that a fork requires a new remote object. The
alternative is to not support them; either way they must not share
defaults with response chaining.

Background execution and batch submission become optional client
interfaces returning pollable handles, consumed by dedicated runners
whose `Output` is the handle type.

## Candidates from review

Directions raised in review of this version, recorded for a later
phase rather than adopted now:

- **Provider-hosted tools.** A `tools:` entry such as Anthropic's
  `web_search` executes provider-side, so it has no handler. A
  `ClientTool` value alongside `Tool` — an interface any class can
  implement, detected by the client and lowered to the wire rather
  than dispatched — is the candidate shape.
- **Richer budgets.** `$max_steps` counts model turns only;
  `$max_time` and `$max_cost` are natural sibling fields on the
  default runner once wall-clock and price metadata are journaled.
- **Tool panic policy.** `on_error` governs thrown failures; whether a
  panicking tool reports or aborts deserves its own knob (`on_panic`)
  when panics become catchable at that boundary.
- **A journal serialization format.** `RunResult.journal` is typed in
  memory; persisting it (and resuming) needs a versioned event format
  and a migration story. This is the entry ticket for phase 4 and
  sessions.
- **`StopReason.Other`.** Wire APIs grow finish reasons; an `Other`
  variant carrying the raw reason would keep unknown stops
  representable instead of collapsing them to `Complete`.

## Sessions

Sessions, steering, and policies return as the sessions draft
describes them, built on this BEP's surface:

- A session is a runner whose journal is durable and addressable, plus
  a persistence layer for journals.
- Session input events (user messages, steering) are new journal event
  types; clients already lower events they recognize and skip the
  rest.
- Policies consume events and issue commands to the loop; the loop's
  primitives (`Journal.new`/`append_all`, `client.invoke`,
  `Tool.call`) are already public.
- Mid-run toolbox changes, budgets beyond `max_steps`, and approval
  flows live in the policy layer.

Specs, clients, content blocks, and the event catalog carry over
unchanged; that is the criterion this BEP was cut against.
