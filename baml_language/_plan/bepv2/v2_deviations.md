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
- Integration testsets are declarations only during this phase. They are not
  run until the user explicitly authorizes live calls.

## Language and runtime improvement backlog

The reference package is also a usability test for BAML itself. These are the
main improvements that would let the final API stay both typed and small:

| Area | Improvement | Removes or simplifies |
| --- | --- | --- |
| Generic types | Default generic arguments plus an existential/erased `Task<T>` form that retains capability evidence before erasure | D-001 and runtime capability matches |
| Generic aliases | Generic type aliases | Repeated inline `Done<T> | BudgetReached | Handoff` unions |
| Capability bounds | A concise way to require several interfaces or name a capability intersection | Custom drivers that need, for example, generation plus transcript import |
| Compiler surface | The reserved top-level `ai` namespace, `$provider`, and declaration-only `.task(...)` selector | D-002 and D-004 manual spellings |
| Function types | BEP-062 `Function`, tuples, named optional parameters, spread/rest, and `reflect.call` | D-009's global JSON dispatcher |
| Runtime reflection | Stable standard JSON Schema from any runtime `type`, with provider adapters allowed to transform the returned `json` | Hand-built provider schemas and unsafe string assembly |
| Media/runtime | Typed audio streams, realtime channel events, and resource cleanup primitives | D-006's text-only live adapter |

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

### D-006: The reference realtime adapter is text-only

**Normative design:** realtime tasks may carry tools and media, and the opened
channel owns interruption and realtime tool-result submission.

**Reference-code spelling:** `OpenAiRealtime` exercises the real WebSocket text
exchange and interruption-shaped resource API, but does not yet translate
`Task.tools` or live audio frames to OpenAI realtime wire events.

**Follow-up:** Add provider-specific realtime tool schemas, tool-result events,
audio append/commit events, and typed channel events before treating the
realtime-tool scenario as a live conformance test.

### D-007: Background and cache providers are deterministic references

**Normative design:** provider adapters implement real background-job and
managed-cache resources when their APIs support them.

**Reference-code spelling:** `FakeBackgroundProvider` and `FakeCacheProvider`
exercise ownership, polling, cancellation, tokens, and cleanup. Their paired
live tests use ordinary OpenAI/Anthropic generation to validate the task and
provider path, not the providers' background/cache wire endpoints.

**Follow-up:** Port OpenAI Responses background jobs and Anthropic prompt-cache
adapters into this temp package, then promote those live tests to resource
conformance tests.

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

### D-009: Function-backed tools still need a separate dispatcher

**Normative design after BEP-062:** `ai.tool(name, description, function)`
retains the function's associated parameter, return, and throws types. Its
erased `Tool.invoke` validates JSON arguments and calls the function through
`reflect.call`.

**Reference-code spelling:** `Tool` stores `parameters: type` and standard
`input_schema: json`, while `AgentOptions.dispatch` is a separate
`(ToolCall[]) -> ToolResult[]` function. Scenario code manually connects tool
names to typed handlers.

**Why:** BEP-062 is proposed but not implemented in the language used by this
reference. Today's BAML lacks the builtin `Function` abstraction, positional
tuples, named optional-parameter tuples, signature-preserving spread/rest, and
the corresponding reflective call.

**Follow-up:** Implement BEP-062, initially restrict `ai.tool` to one required
class argument, and move dispatch into the function-backed `Tool`
implementation. Keep runtime-schema MCP tools and provider-owned tools as
separate implementations; they do not require BAML function values.

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
