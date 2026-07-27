# BEPv2 Reference-Code Deviations

This file records places where `crates/baml_tests/baml_src_temp` cannot yet
spell the normative BEPv2 API exactly in today's BAML language or runtime.
It is an implementation ledger, not an alternative design. Each entry should
either become compiler/runtime work or be removed when the reference code can
match the design.

## How to read the reference package

- `ns_ai/` is the proposed shared `ai` namespace.
- `ns_ai_scenarios/` follows the user-guide themes rather than the historical
  numbered provider-scenario list.
- Every LLM-function example has a hand-written `Name_task(...)` companion.
  The comment immediately above it shows the future `Name.task(...)` lowering.
- Integration testsets remain opt-in because they make live provider calls.
  The reconciliation run executes them only when credentials are available
  and reports credential/provider failures separately from offline failures.

## Language and runtime improvement backlog

The reference package is also a usability test for BAML itself. These are the
main improvements that would let the final API stay both typed and small:

| Area | Improvement | Removes or simplifies |
| --- | --- | --- |
| Generic types | Default generic arguments plus an existential/erased `Task<T>` form that retains capability evidence before erasure | D-001 and runtime capability matches |
| Generic aliases | Generic type aliases | Repeated inline `Done<T> | BudgetReached | Handoff` unions |
| Capability bounds | A concise way to require several interfaces or name a capability intersection | Custom drivers that need, for example, generation plus transcript import |
| Compiler surface | The reserved top-level `ai` namespace, `$provider`, and declaration-only `.task(...)` selector | D-002 and D-004 manual spellings |
| Function types | Complete BEP-062 ergonomics with parameter documentation and safe explicit array widening | Manual parameter descriptions and `tool_inputs_from_tools` at invariant-array boundaries |
| Runtime reflection | Stable standard JSON Schema from any runtime `type`, with provider adapters allowed to transform the returned `json` | Hand-built provider schemas and unsafe string assembly |

These are language/runtime opportunities, not reasons to move lifecycle policy
into the compiler. `.task(...)` remains the only LLM-function companion.

## Open deviations

### D-001: The executable task erases its provider type

**Normative design:** `Task<T, P extends Provider = Provider>` retains `P`, so
drivers can require capabilities at compile time.

**Reference-code spelling:** `Task<T>` stores `provider: Provider`. Drivers use
an explicit runtime interface match before dispatch.

**Why:** Today's parser does not accept a default generic type argument, and an
interface such as `Provider` cannot be supplied where a bounded generic needs a
concrete runtime type. This prevents the intended ergonomic default and erased
form from coexisting in ordinary BAML source.

**Follow-up:** Add the compiler/runtime representation needed for existential
provider erasure while retaining concrete capability evidence before erasure.

### D-002: The task field is `provider`, not `$provider`

**Normative design:** compiler-generated tasks store the selected provider in
the reserved `$provider` field.

**Reference-code spelling:** hand-written tasks store it as `provider`.

**Why:** `$provider` is proposed compiler-owned syntax. The temp package is
ordinary BAML and deliberately does not implement LLM-function desugaring.

**Follow-up:** Generate the reserved field when `.task(...)` lowering lands.

### D-003: Capability checks happen at driver runtime

**Normative design:** a `Task<T, P>` passed to a safe driver proves that `P`
implements the required provider capability.

**Reference-code spelling:** safe-reference drivers match the erased provider
against `DriveProvider`, `GenerationProvider`, or the relevant resource
interface and throw `baml.errors.Unsupported` when it does not match.

**Why:** This follows directly from D-001. The scenarios keep safe and unsafe
driver names separate so their intended contracts remain visible.

### D-004: User namespaces are rooted under `root`

**Normative design:** `ai` is a compiler-provided top-level namespace parallel
to `baml` and `assert`, so user code writes `ai.drivers.drive(...)`.

**Reference-code spelling:** the ordinary `ns_ai/` directory produces
`root.ai`; themed scenarios therefore write `root.ai.drivers.drive(...)`.

**Why:** The temp package intentionally uses only today’s namespace mechanism.
Making `ai` a reserved top-level namespace is separate compiler/stdlib work.

### D-005: `AgentRun<T>` is written as an inline union

**Normative design:** `type AgentRun<T> = Done<T> | BudgetReached | Handoff`.

**Reference-code spelling:** driver signatures use
`Done<T> | BudgetReached | Handoff` directly.

**Why:** Today's BAML type aliases cannot declare generic parameters. The
concrete outcome classes and their matching behavior are otherwise identical.

### D-006: Realtime caller-audio input is wired

**Normative design:** realtime tasks may carry tools and media, and the opened
channel owns interruption and realtime tool-result submission.

**Reference-code spelling:** `LiveSession.send_audio` streams caller PCM frames,
`commit_audio` ends manually delimited turns, and `send_audio_turn` is the
bounded convenience. `OpenAiRealtime.server_vad` enables provider-owned speech
boundaries, response creation, and interruption without moving session
ownership out of the `LiveSession` resource.

### D-008: Provider prompt rendering uses a shorthand string

**Normative design:** `Provider.prompt_context(output_type)` supplies pure,
provider-sensitive prompt rendering, while `descriptor()` supplies display and
diagnostic data that is never treated as provider identity.

**Reference-code spelling:** `Provider.render_shorthand()` returns a value such
as `"openai/gpt-5.6-luna"`. `PromptRenderRecipe` converts it with
`baml.llm.from_shorthand` and `baml.llm.build_prompt_context`.

**Why:** Today's public BAML APIs expose the primitive prompt-context builder
through an LLM-client shorthand. The temp package is deliberately implemented
entirely in BAML and cannot construct a richer provider-neutral render adapter.

**Follow-up:** Expose a stable prompt-context construction contract to custom
providers. Keep it pure and separate from execution capabilities; do not make
display names or descriptors into equality keys.

### D-009: Function-backed tool dispatch is resolved

**Normative and reference design after BEP-062:** bare function references are
the default configuration API:

```baml
Agent.new(tools = [search_knowledge, lookup_account])
```

The public `ToolInput` union accepts either a function or an already-normalized
`Tool`. The runner derives the function's declared name, docstring, parameter
schema, and handler from `reflect.signature`, then dispatches checked named
arguments with `reflect.call_any`. No separate name-based dispatcher remains.

`ai.tool(handler, name = ..., description = ...)` remains only as the explicit
escape hatch for aliases, anonymous closures, description overrides, handoffs,
and runtime/MCP schemas. Provider-owned tools remain separate provider
configuration and do not acquire application handlers.

### D-010: Provider-owned tool removal uses a deterministic provider

**Normative design:** provider-owned tools are typed provider configuration.
Removing them between agent steps selects a derived provider value with those
tools disabled; `StepPlan.tools: []` independently clears application tools.

**Reference-code spelling:** the executable removal scenario uses
`FakeToolProvider.provider_tools` and switches to a same-name provider value
whose list is empty. `FakeToolTranscript` records application and
provider-owned rosters separately so the assertions can prove both were
removed.

**Why:** the temp OpenAI adapter currently demonstrates application function
tools through Chat Completions but does not yet implement an actual
provider-executed web-search/code-execution configuration. The fake isolates
the ownership and provider-identity rule without making a live request.

**Follow-up:** add typed provider-owned tool classes to the real OpenAI and
Anthropic adapters, serialize them through those providers' native APIs, and
pair the deterministic scenario with live conformance tests. The application
and provider tool lists must remain separate even if a vendor serializes them
into one wire-level array.

### D-011: Prompt/SAP tool capability is lifted at runtime

**Normative design:** a concrete provider that supports prompt/SAP tool turns
implements `ToolCallingProvider` explicitly, potentially through a reusable
out-of-body implementation. `run_agent` retains `Task<T, P>` and statically
requires that capability.

**Reference-code spelling:** today's executable `Task<T>` erases its provider
type. `run_agent` first selects an existing `ToolCallingProvider`; otherwise it
wraps a `GenerationProvider` in `PromptToolProvider` at runtime. The wrapper
re-renders the task recipe with `ctx.output_format` for `T | ToolCalls` and the
active tools' JSON Schemas on every step.

**Why:** the temp package cannot express `Task<T, P>` or the associated
capability-preserving generic adapter yet. The runtime lift exercises the
intended requests, union parsing, dispatch, and transcript behavior without
weakening the normative static API.

**Follow-up:** preserve `P` in `Task<T, P>`, provide a standard reusable
prompt-tool implementation that concrete providers can adopt out of body, and
make the safe driver reject providers that declare neither native nor
prompt-backed `ToolCallingProvider`. Keep the unsafe runtime-negotiated driver
as the explicit erased escape hatch.

### D-012: `stream_agent` needs a real incremental runtime

**Normative design:** `stream_agent` yields lifecycle, tool, usage, provider,
and partial-output events as they occur.

**Reference-code spelling:** `run_agent` publishes the same lifecycle events
to hooks, observers, and recorders, but the temp package does not expose a
buffered object under the misleading name `stream_agent`.

**Why:** the current `ToolCallingProvider.step` boundary is request/response.
Returning an eagerly populated event list would compile but would not provide
streaming semantics.

**Follow-up:** add an incremental agent-event transport that can multiplex
provider deltas with tool lifecycle events, then implement both `Task` and
`StreamTask` overloads.

### D-013: Realtime tasks use `null`

**Normative design:** `open_live` accepts `Task<null>`. The task supplies
instructions, arguments, tools, and provider selection; the non-generic `LiveSession`
resource supplies events and controls. A driver must not accept `Task<T>` when
no successful terminal path exposes `T`.

**Reference-code spelling:** `VoiceSupport_task` returns `Task<null>`, and the
public `open_live` resource operation accepts only `Task<null>`. The provider's
internal opener remains generic implementation plumbing, but callers cannot
discard an arbitrary task result through the public helper. A future bounded
realtime operation should use a distinct `LiveRun<T>` contract with an explicit
terminal result instead of making open-ended `LiveSession` generic.

## Closed deviations

### D-007: Real background jobs and managed caches

The reference now has a real OpenAI Responses background provider and a real
Gemini managed-cache provider. Live tests submit and resume remote background
work, create and use a provider cache, and verify its remote deletion after
resource cleanup. Private provider fixtures remain useful for fast
deterministic conformance tests, but they are not proposed public APIs.
