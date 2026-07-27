# Reference implementation differences

The executable design prototype lives in
[`crates/baml_tests/baml_src_temp2`](../../../crates/baml_tests/baml_src_temp2).
It proves many of the runtime ideas in this proposal, but it is not yet the
proposed public API.

This page is the reconciliation contract. Public examples show the intended
surface. This page says where the prototype still differs and whether the BEP
or the implementation should win.

## Already aligned

These parts of the prototype should be carried forward:

| Area | Current evidence |
| --- | --- |
| Runner protocol | [`Runner<Input>`](../../../crates/baml_tests/baml_src_temp2/ns_ai/core/runner.baml) has associated `Output` and `Error` |
| Stateful runner values | Agent, background, batch, stream, transcription, voice, and harness runners keep configuration on classes |
| Inline implementations | Standard runner classes implement `Runner` inside their class bodies |
| Tool functions | [`ToolInput`](../../../crates/baml_tests/baml_src_temp2/ns_ai/tools/models.baml) accepts `baml.AnyFunction` and configured `Tool` values |
| Tool defaults | Reflection derives optional parameters and `reflect.call_any` applies BAML defaults |
| Agent ownership | [`run_agent`](../../../crates/baml_tests/baml_src_temp2/ns_ai/tools/agent.baml) owns application dispatch, hooks, limits, events, and termination |
| Provider turns | `ToolCallingProvider` exposes `begin`, `step`, and `submit` |
| Messages and state | `Message`, `Messages`, and `Conversation` are interfaces; `MessageHistory` is editable application data |
| Provider capabilities | Completion, generation, streaming, tool calling, background, batch, session, transcription, and realtime are separate interfaces |
| Raw realtime | [`open_live`](../../../crates/baml_tests/baml_src_temp2/ns_ai/resources/realtime_provider.baml) is already a direct resource operation |
| Task-first transcription | `Transcribe` and `TranscribeWithMeta` accept a typed `TranscriptionTask`; the task owns the provider and finite audio input |
| Observability | Agent events distinguish model, roster, provider, tool, usage, and terminal events |
| Live scenarios | The scenario tree includes real OpenAI, Anthropic, background, cache, realtime, and callback-observed Claude Code harness paths |

## Required compiler lowering

### LLM function companions

The prototype hand-writes functions such as:

```baml
ResolveTicket_task(ticket, provider)
ResolveTicket_manual(ticket, provider)
```

See
[`00_shared/models.baml`](../../../crates/baml_tests/baml_src_temp2/ns_ai_scenarios/00_shared/models.baml)
and
[`00_declared_tools_and_direct_calls.baml`](../../../crates/baml_tests/baml_src_temp2/ns_ai_scenarios/02_tools_and_agents/00_declared_tools_and_direct_calls.baml).

The BEP wins. The compiler must generate:

```baml
ResolveTicket.task(ticket)
ResolveTicket(ticket)
```

It must capture `provider:`, `prompt:`, `tools:`, function identity, arguments,
and the declared return type without requiring a second hand-written task
function.

### Direct-call ownership

Today, `OpenAi.complete` and `Anthropic.complete` inspect `task.tools` and call
`complete_with_bounded_agent`. That makes a provider completion method appear
to own the BAML application-tool loop:

- [`OpenAi.complete`](../../../crates/baml_tests/baml_src_temp2/ns_ai/providers/openai/provider.baml)
- [`Anthropic.complete`](../../../crates/baml_tests/baml_src_temp2/ns_ai/providers/anthropic/provider.baml)
- [`complete_with_bounded_agent`](../../../crates/baml_tests/baml_src_temp2/ns_ai/tools/agent.baml)

The BEP wins. Compiler/runtime `run_direct(task)` selects `Completion` or
`Agent`. Provider `complete` remains a bounded provider operation and never
starts application dispatch.

The manual scenario already demonstrates the desired branch. It should become
a compiler regression rather than permanent application code.

## Type-system migrations

### Preserve the provider type

The prototype uses:

```baml
class Task<T> {
  provider: Provider,
}
```

and `with_provider(provider: Provider) -> Task<T>`. See
[`core/task.baml`](../../../crates/baml_tests/baml_src_temp2/ns_ai/core/task.baml).
This erases capability proof and forces runtime `match` checks inside runners.

The BEP wins:

```baml
Task<T, P extends Provider = Provider>
```

`.task(...)` should infer the concrete `P`; `.with_provider(next)` should return
`Task<T, Next>`. A deliberately erased `Task<T>` remains possible at dynamic
boundaries, but it is not the default inference result.

### Infer runner type arguments

The prototype normally spells:

```baml
ai.run.Agent<Resolution>.new(...)
ai.run.Completion<Resolution>.new()
```

The BEP uses:

```baml
ai.run.Agent.new(...)
ai.run.Completion.new()
```

The BEP wins. `Task.run` supplies enough input context to infer `T`, provider
type, and associated output/error projections. Explicit type arguments remain
available when inference is genuinely ambiguous.

### Preserve typed task arguments for specialized runners

The prototype's ordinary `Task<T>` stores generated arguments as
`map<string, unknown>`, which cannot preserve a media handle through a JSON
cast. It therefore hand-writes `TranscriptionTask` as the function-specific
task type used by the transcription scenario. This proves the task-first call
shape and prevents an unrelated `Task<string>` from type-checking, but the
compiler does not generate that type yet.

The BEP wins. A generated task must retain its captured arguments in a typed
form that a compatible specialized runner can access without serialization,
string keys, or unchecked casts. `TranscribeAudio.task(audio)` remains a task,
and `Transcribe` is a `Runner` only for a compatible finite-audio task shape.

### Type provider-owned conversations

The prototype's `Conversation` is an unparameterized interface whose
`provider()` method returns erased `Provider`. On resume, `run_agent` silently
chooses `existing.provider()` instead of checking the task's selected provider.

The BEP wins:

```baml
Conversation<P>
```

The Agent must reject a task/provider and conversation-owner mismatch before
I/O. Switching providers uses explicit message export/import.

## Public API migrations

The prototype's `with_provider(...)` method and hand-written `$provider`
override already use the target terminology. Compiler-generated task
companions should retain those spellings.

| Prototype | BEP target | Decision |
| --- | --- | --- |
| `Budget { max_steps, max_cost_usd }` | fields on `Agent.new(...)` | Flatten the common settings |
| raw `Done<T> \| BudgetReached \| Handoff` | `AgentOutcome<T>` alias | Add the readable alias |
| `LoopBudgetExceeded` / `HandoffUnresolved` | `AgentIncomplete` with terminal outcome | Preserve one resumable direct-call error contract |
| `root.ai.fake_output_provider(json_string)` | Private reference-suite fixture | Do not add it to the public `ai` surface |
| `OpenSession.new().run(provider)` | `ai.open_session(provider, ...)` | A raw provider resource is a direct operation |
| `CreateCache.new(...).run(provider)` | `ai.create_cache(provider, ...)` | A raw provider resource is a direct operation |
| `SaveConversation` / `RestoreConversation` runners | provider save/restore operations | They do not consume a task and do not change task output shape |
| provider-wrapper `retry(...)` / `fallback(...)` | `ai.run.Retry` / `ai.run.Fallback` | Reliability wraps a lifecycle and preserves its associated output |
| only homogeneous `Batch<T>` | homogeneous `Batch<T>` plus typed `BatchQueue` item handles | Keep the safe simple case and add heterogeneous submission without `T[]` erasure |

## Behavior differences

### Prototype-only test fixtures

The classes and helpers under
[`ns_ai/testing`](../../../crates/baml_tests/baml_src_temp2/ns_ai/testing) exist
only to test the executable prototype. They are not proposed standard-library
APIs and must not appear in user-facing namespace documentation.

The final conformance suite may keep equivalent fixtures in test-only source.
Applications may also write their own provider implementations when useful,
but BAML does not reserve or publish a standard testing namespace for them.

### Tool registry duplicates

The prototype rejects every duplicate name, including adding the same
function twice. Its existing scenario asserts that behavior.

The BEP treats adding the same function identity under the same derived name
as idempotent. A different handler with the same name still fails, and
replacement stays explicit. The BEP wins because setup code can safely
reconcile a known roster without weakening collision safety.

The implementation and
[`06_dynamic_tool_registry.baml`](../../../crates/baml_tests/baml_src_temp2/ns_ai_scenarios/02_tools_and_agents/06_dynamic_tool_registry.baml)
must change together.

### `tools` and `tool_registry`

The prototype already makes an explicit registry authoritative by choosing it
instead of `tools ?? task.tools`, but it silently ignores a non-null `tools`
argument when both are present.

The BEP keeps the authoritative-registry behavior and adds a construction
error for the ambiguous combination. This should be tested.

### Stable provider identity

The prototype commonly uses `Provider.name()` strings in tokens and ownership
checks. Names are suitable for logs but may collide across endpoint, account,
deployment, or injected transport boundaries.

The BEP's stable provider descriptor wins. Tokens and conversations must carry
that identity; display labels remain separate.

### Response and Agent metadata

The prototype's `Response<T>` and `Done<T>` already place usage under `meta`.
Keep that shape. `BudgetReached` and `Handoff` should gain terminal `meta` as
well so every Agent exit reports cumulative usage consistently.

### Implementation placement

The standard runner classes already keep configuration and `Runner`
implementations together. Some owned provider and harness classes still use
detached `implements` blocks, and one scenario explicitly demonstrates that
spelling.

For a class designed to provide a capability, the BEP's inline style wins:
fields, constructor, and capability methods should be readable together.
Detached implementations remain valid when adapting a type owned elsewhere or
when a genuinely separate package supplies the conformance.

## Resource lifecycle gap

The prototype's [`Resource`](../../../crates/baml_tests/baml_src_temp2/ns_ai/core/resource.baml)
interface is empty. `cleanup()` implementations are commented out on jobs,
batches, caches, sessions, harness sessions, realtime sessions, and audio
devices.

The BEP wins:

- resources expose explicit idempotent domain operations such as `close`,
  `cancel`, and `delete`;
- the special `cleanup()` function is a GC fallback;
- explicit operations and cleanup share one release guard; and
- deterministic tests can request a full GC cycle and wait for eligible
  cleanup functions.

This requires compiler/runtime cleanup support before the resource examples
can become literal conformance fixtures.

## Surface not implemented yet

The following public examples are intentional target API, not claims about the
current prototype:

- LLM declaration `tools:` and generated `.task(...)`;
- `Task<T, P>` provider typing;
- `Task.with_messages(...)`;
- `AgentOutcome<T>` and `AgentIncomplete`;
- runner-valued retry and fallback;
- `ai.open_session` and `ai.create_cache`;
- provider-owned hosted-tool configuration in bounded completion;
- heterogeneous `BatchQueue` with typed item handles;
- stable provider descriptors;
- live MCP connection/resource helpers used by the discovery guide; and
- deterministic GC triggering for cleanup tests.

When one of these becomes executable, move its scenario from a conceptual
fixture to a type-checked and behavior-checked regression.

## Scenario work still needed

The prototype has broad scenario coverage, but the BEP's conformance matrix
adds these requirements:

| Gap | Required regression |
| --- | --- |
| Generated LLM companions | Compile and run `.task(...)` without a hand-written helper |
| Direct tool call | Prove the compiler-owned direct path uses Agent and providers do not start it |
| Provider type preservation | Reject Agent/Stream/Background with incompatible concrete `P` |
| Associated-type inference | No explicit `<T>` and no inferred `<unknown>` in standard runner calls |
| Registry ambiguity | Reject non-null `tools` together with `tool_registry` |
| Resume identity | Reject a same-name but different-owner conversation |
| Agent terminal metadata | Usage present on done, budget, and handoff exits |
| Heterogeneous batch | Two item handles recover two unrelated result types |
| Cleanup fallback | Trigger GC and assert exactly-once release |
| Provider-owned tools | Live bounded provider operation uses a hosted tool without BAML dispatch |
| MCP bootstrap lifetime | Generated tools remain callable until the owning connection resource closes |
