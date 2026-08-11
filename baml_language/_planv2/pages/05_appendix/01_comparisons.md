# Comparisons

Each section records what a system does, what this BEP adopted from
it, and what it deliberately does differently.

## pi

pi's SDK separates provider descriptors from reusable wire API
implementations: a provider declares identity, authentication, and
model metadata, and binds an API implementation such as
`openAICompletionsApi()`. An OpenAI-compatible service reuses the API
implementation with different configuration. This BEP keeps the idea
and drops the public descriptor: the registry prefix plus a factory is
the descriptor, and configuration-over-codec is a convention rather
than a second interface.

pi's normal OpenAI Responses adapter sends complete input with
`store: false` and keeps its local context sufficient for resume;
server-held continuation exists only in a specialized adapter, guarded
by compatibility checks, and is discarded when unusable. This BEP
adopts the ordering as law: the journal alone renders a complete
request, and server state arrives later as a discardable optimization.

pi retains API-native replay data as narrow per-block signatures — a
serialized reasoning item, an output message id — never the whole HTTP
response. This BEP's phase 2 replay capsule is the same judgment
stored per turn.

pi's continuation logic lives in the wire adapter, which knows which
request fields must match and which errors mean a cursor is dead. This
BEP's single public `invoke` exists so that the same knowledge can
stay inside the client.

The anti-lesson is pi's `sessionId`, which serves prompt-cache keying,
connection affinity, and continuation state at once. The future-phase
design keeps those as separate values with separate lifetimes.

Finally, pi carries TypeBox schema declarations alongside its
TypeScript types because erased types cannot render schemas or
validate at runtime. BAML types are runtime values, so one declaration
is the check, the schema, and the validator; this is why `tools:`
takes plain functions.

## Pydantic AI

Pydantic AI persists conversation state as provider message arrays.
Message arrays tie state to one provider's wire format, lose tool and
usage structure, and make cross-provider resume a lossy conversion.
This BEP records typed events and renders them per client, so
switching providers is a rendering decision. The shared ground is
typed outputs validated by schemas derived from the host language's
types.

## OpenAI Agents SDK

The Agents SDK organizes work around an agent object bound to a
provider, with handoffs between agents and a run loop inside the SDK.
This BEP has no agent object: the function is the declaration, the
spec is the run currency, and the loop lives in a replaceable runner.
Handoffs and multi-agent composition are ordinary code that runs one
spec and then another, and the session and policy layers that make
that richer are deferred (`03_future_phases.md`).

## BEPv4 (`begin`/`step`/`submit`)

The earlier BAML v4 design routed every run through a provider-owned
mutable `Conversation` with `begin`, `step`, and `submit` methods, and
providers implemented a documented atomicity discipline — mutate state
only after wire success — across every method. The reference providers
were 2,000–3,000 lines each, roughly 70% internal plumbing, with the
prompt-mode tool adapter, schema walkers, correlation checks, and the
HTTP send/classify block duplicated per provider.

Adopted from v4: the failure taxonomy (the classified vocabulary,
`classify_http`, and the replay-safety fact — v4's `Effects`, here
named `RetrySafety`), the runner-owned loop and tool
execution, tool results as data, capability-by-interface with
`Unsupported` as the uniform rejection, and reliability as wrapper
values.

Dropped: the conversation lifecycle (state lives in the journal; the
atomicity contract became structural because `invoke` returns a value
and the runner commits), provider-generic `step<T>` (clients never see
the output type's parse), pre-rendered prompts in the task value
(rendering is per turn, per client), and bound-method identity checks
through `delegate()` chains (nothing is owned, so nothing is
checked). The v4 `Task<T>` became `FunctionSpec<Out>` with the
rendering removed.

## The sessions draft (`_plan/`)

The sessions draft (`../../../_plan/`) defines sessions, steering,
policies, jobs, and serving over the same journal idea. This BEP keeps
its journal-owns-state law and its event style, narrows assistant
content from string-plus-`raw_json` to structured blocks as that
draft's own appendix recommended, and collapses the provisional
`render`/`invoke`/`ingest` client contract to one `invoke`. Sessions,
steering, and policies are deferred, not rejected; the re-entry path
is recorded in `03_future_phases.md`.
