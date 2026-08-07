# BEP: LLM functions, specs, runners, and clients — Outline

Full page and header map. Pages are written against this outline;
update both in the same change. Refer to sections in review as
`guides/03_clients/02 The client interface > Statelessness`.

```
_planv2/
├── outline.md
├── readme.md         ← summary, the flow diagram, the API tree, reading order
├── style.md          ← prose and structure rules for pages in this BEP
└── pages/
    ├── 01_introduction/
    │   ├── 01_getting_started.md
    │   ├── 02_why.md
    │   └── 03_concepts.md
    ├── 02_guides/
    │   ├── 01_functions/
    │   │   ├── 01_llm_functions.md
    │   │   ├── 02_tools.md
    │   │   └── 03_calling_functions.md
    │   ├── 02_specs_and_runners/
    │   │   ├── 01_specs.md
    │   │   ├── 02_the_default_runner.md
    │   │   └── 03_writing_a_runner.md
    │   ├── 03_clients/
    │   │   ├── 01_choosing_a_model.md
    │   │   ├── 02_the_client_interface.md
    │   │   ├── 03_writing_a_client.md
    │   │   ├── 04_reliability.md
    │   │   └── 05_the_built_in_clients.md
    │   └── 04_the_journal.md
    ├── 03_how_to/
    │   ├── readme.md
    │   ├── 01_retry_a_failed_parse_with_feedback.md
    │   ├── 02_test_without_a_network.md
    │   └── 03_use_a_local_model.md
    ├── 04_reference/
    │   ├── 01_api.md
    │   ├── 02_events.md
    │   └── 03_errors.md
    └── 05_appendix/
        ├── 01_comparisons.md
        ├── 02_alternatives_considered.md
        └── 03_future_phases.md
```

## Introduction

### 01_getting_started.md (tutorial)

- A typed LLM call — `ExtractRecipe`; the return type is the schema;
  `${ctx.output_format}`.
- Make it an agent — add `tools:`; the same call now loops.
- What a call does — the desugar to `Agent.run(MyFunc@spec(...))`;
  one-turn functions use the same loop.
- Inspect the run — `@spec` plus `run()` returns `RunResult`; read the
  journal and usage.
- Point it at another model — change the `client:` string; pass
  `$client` at the call site.
- Where to go next.

### 02_why.md (explanation)

- The problem — untyped prompts, per-provider SDK divergence, agent
  loops rewritten per application.
- The approach — one typed function form; specs, runners, and clients
  as plain values; the journal as the single record.
- What you do not get — no sessions or steering yet, no policies, no
  background jobs, no graph DSL; pointer to
  `../05_appendix/03_future_phases.md`.
- Relation to other systems — one paragraph each; details in
  `../05_appendix/01_comparisons.md`.

### 03_concepts.md (explanation)

- The pieces — function, spec, runner, client, journal, tool; one
  sentence each.
- The turn loop — the readme diagram, walked through once.
- Who owns what — runner: loop, tools, correlation, final parse;
  client: rendering, transport, normalization; journal: all state.
- The two laws — assistant content is structured blocks; the journal
  alone renders a complete request.
- Glossary — the terms the style guide holds pages to.

## Guides

### 01_functions/01_llm_functions.md

- An LLM function is a typed function — prompt body, return type,
  `client:` field.
- The prompt is the instructions — rendered fresh each turn; the
  conversation lowers as messages after it; `${ctx.output_format}` is
  the one placeholder; the first-turn mapping belongs to the client.
- The return type is the contract — parsing and repair happen before
  user code sees the value.
- Media arguments and outputs — media parameters lower as parts; a
  return type of exactly `image`/`image[]` binds media blocks in
  phase 2; media nested in structured output is rejected.
- Defaults in the function block — `client:`, `tools:`; how call sites
  override them.

### 01_functions/02_tools.md

- The `tools:` field — a list of plain functions; no separate agent
  declaration.
- A tool is a function — schemas from signatures and docstrings via
  reflection; the explicit `tool(...)` constructor.
- Argument validation — malformed calls become tool errors, not
  crashes.
- Tool errors are data — a failed tool returns to the model as a
  result; the application `throws` channel is not involved.
- Tool failure policy — `Report` is the default; `Raise` ends the run
  by throwing `ToolFailedError` after the failure is journaled. Set it
  per tool with `tool(fn, on_error = Raise)` or for the whole run with
  `$tool_errors = Raise`; the per-tool setting wins.
- Parallel calls — concurrency within a turn; correlation by call id.
- Changing the toolbox — the `tools:` list is static; a different
  toolbox means a custom runner; mid-run changes arrive with sessions.

### 01_functions/03_calling_functions.md

- Calling runs the default runner — the desugar; `.value` unwraps.
- One-turn functions — a function without `tools:` completes on the
  first turn; same journal shape, same errors.
- `$` parameters — `$` names set fields on the default runner, bare
  names go to the function; the catalog: `$client`, `$max_steps`,
  `$tool_errors`, `$on_event`.
- Switching the client — `$client` is the one override; running one
  function across several providers is a loop over client values.
- There is no `runner:` field — the function block is a static
  template; a different runner is an explicit
  `my_runner.run(MyFunc@spec(...))` call.
- Step budgets — the default of 12 model turns;
  `StepBudgetExceeded`.
- Errors at the call site — what throws, what returns; `catch_all`
  examples.
- Every call is recorded — the journal exists even when you never
  read it; how to get it when you want it.

### 02_specs_and_runners/01_specs.md

- What `@spec` creates — bind arguments, return a value, call no
  model.
- A spec is a recipe — no journal, no conversation, no wire state;
  runnable any number of times; the prompt is not pre-rendered.
- Reading a spec — `name`, `arguments`, `output_type`, `prompt`,
  `tools`, `default_client`.
- Specs are read-only — getters only; every override lives on the
  runner that consumes the spec.
- What specs are for — custom runners, evals across clients, later
  serving registries; ordinary code never sees one.

### 02_specs_and_runners/02_the_default_runner.md

- The `Agent` runner — fields, defaults, construction; `$` parameters
  at a call site set these fields.
- The turn loop, step by step — assemble the turn input, invoke,
  commit the turn atomically, execute tools, append results, repeat.
- The correlation invariant — every `ToolUse` id receives exactly one
  result before the next model turn.
- Final parsing — the client normalizes a candidate; the runner runs
  the schema-aware parser; the repair loop re-asks within the step,
  committing every attempt to the journal; the same loop is writable
  from public primitives.
- `RunResult` — value, journal, usage.
- Observing a run — `on_event`; deltas are out of scope until
  streaming.

### 02_specs_and_runners/03_writing_a_runner.md

- The `Runner` interface — the associated `Output` and `Error` types;
  why both vary by runner; `run` never throws untyped; no required
  fields, and the embed-an-`Agent` convention for shared options.
- The building blocks — the public primitives, with no intermediate
  helpers: `Journal.new`/`append_all`, `client.invoke` over assembled
  materials, `Tool.call`, and `baml.sap.parse<T>`.
- Example: a wrapping runner — retry or budget logic around an inner
  runner.
- Example: an eval runner — one spec, several clients, scored results;
  pointer to the how-to recipes.
- What a runner must uphold — atomic commits of terminal turns; the
  correlation invariant; no partial state on failure.

### 03_clients/01_choosing_a_model.md

- Model strings — `client: "openai/gpt-5.6"`; the prefix selects an
  implementation, the rest configures it.
- Resolution — the registry; built-in prefixes; where credentials come
  from.
- Same implementation, different model — `openai/gpt-5.6` and
  `openai/gpt-5.5` are one class, two field values.
- Registering a prefix — `clients.register`; an OpenAI-compatible
  endpoint as configuration, not code.
- The one override — `$client` at the call site; `Agent { client: }`
  is the same setting at the explicit layer, not a second way.
- Constructing and deriving clients — clients are plain values;
  `resolve` is a convenience over `new`, which defaults every
  parameter including the environment credential; a class literal
  gives full control; spread an existing client to change one option.
  There is no separate client-registry mutation API.

### 03_clients/02_the_client_interface.md

- One operation — `id()` and `invoke(ModelTurnInput) -> ModelTurn`;
  why render and ingest are not public.
- `ModelTurnInput` — prompt, journal, toolbox, output type; materials,
  not renderings.
- The client owns the transformation — output-contract placement,
  `ctx.output_format` dialect, tool lowering, transcript lowering.
- `ModelTurn` and content blocks — `Text`, `Reasoning`, `ToolUse`,
  `Media`; the final candidate; `StopReason`.
- Statelessness — every request rebuilt from the input; what this buys
  (retry, forking later, no ownership checks).
- What a client never does — journal writes, tool execution, typed
  output parsing, loop control.

### 03_clients/03_writing_a_client.md

- The anatomy — a config class, a pure render function, a shared
  transport call, a pure parse function.
- Rendering the prompt — supplying the output-format text or placing
  the contract on the wire instead.
- Lowering tools — wire dialects; the shared schema helpers.
- Lowering the journal — roles, tool results by call id, foreign
  clients' turns from canonical blocks, synthesized call ids.
- The wire library — `send_as<T>`, classification, schema walkers
  layered over `baml.schema.json_schema` and `baml.http`; what stays
  per-client; dropping to the primitives.
- Testing a client — literal journals in, turns out; `ScriptedClient`
  for loop tests.

### 03_clients/04_reliability.md

- The error model — errors carry facts, callers make judgments;
  `RetrySafety` is the fact only the failing layer knows.
- The error catalog — pointer to `../../04_reference/03_errors.md`;
  the classified vocabulary in one table.
- Retry — the wrapper client; what is safe to retry and why;
  `retry_after_ms` hints.
- Fallback — the wrapper client; member advance rules; re-rendering is
  automatic because every invoke re-renders.
- HTTP classification — `classify_http`; the status table.
- Reading the provider's error response — `raw_body` on classified
  wire failures; headers require `baml.http.send` directly.

### 03_clients/05_the_built_in_clients.md

- The two representation choices — the `output_mode` field (ships as
  `Sap`; `Native`/`Strict` in phase 2) and tool lowering (native by
  default; `PromptTools` wrapper); the axes compose without
  cross-field validation.
- Rules shared by every client — schemas derive from
  `reflect.signature` into `Tool.input_schema`; execution is the
  runner's `reflect.call_any`, never the client's; the per-turn
  decision; uniform result serialization; no argument repair; the
  reserved name; reasoning as readable projection only.
- OpenAI (`OpenAiClient`) — Responses with `store: false`; concrete
  request and response bodies; phase 2 strict schemas and the reserved
  result function; phase 3 chaining.
- Anthropic (`AnthropicClient`) — system plus messages; interleaved
  content blocks including thinking; `output_config` composes with
  tools, so no reserved function.
- Google (`GoogleClient`) — `generateContent`; instructions as the
  leading user content on every turn, never `systemInstruction`;
  synthesized call ids; `responseJsonSchema` versus the
  reserved-function fallback when tools are present.
- Media lowering — the per-provider argument table; rejected cells
  throw `Unsupported`; media output normalization is phase 2.
- Claude Code (`ClaudeCodeClient`) — a harness client over the CLI as
  a local process; contract native via `--json-schema`; BAML tools via
  the `outcome` envelope; `session_id` as a phase 3 checkpoint.
- Prompt-mode tools are a wrapper, not a mode — the phase 2
  `PromptTools` wrapper client; empty toolbox for the inner client;
  the calls-envelope rewrite; the discriminator caution.

### 04_the_journal.md

- What the journal is — the append-only record of one run; the
  transcript source and the trace, one structure.
- Built-in events — the catalog in brief; produced by the runner only.
- `AssistantMessage` — content blocks, the producing client's id.
- Reading a run — walking `RunResult.journal`; usage accounting.
- What is not recorded — token deltas; raw HTTP envelopes; where those
  live instead.

## How-to

One task per page. Recipes are short and carry no H2 headers; each
entry notes the task.

- `readme.md` — what belongs in this section; the page list.
- `01_retry_a_failed_parse_with_feedback.md` — the feedback loop from
  public primitives: `spec.prompt()`, `Journal.append_all`,
  `UserMessage`, `client.invoke`; every attempt is committed.
- `02_test_without_a_network.md` — `ScriptedClient` drives the loop;
  assert on `received()`; tools still execute for real.
- `03_use_a_local_model.md` — register a prefix over the OpenAI codec
  or pass a client value; pointer to the `PromptTools` phase 2
  wrapper.

## Reference

### 01_api.md

- The tree — the readme tree, restated.
- `FunctionSpec<Out>` — every method, signature, throws.
- `Runner<Out>` and `Agent<Out>` — fields, associated types,
  signatures.
- `RunResult<Out>`.
- `Client`, `ModelTurnInput`, `ModelTurn` — the interface and the
  turn contract.
- Content blocks and `StopReason` — shapes and validity rules.
- `Prompt` — the render surface a client calls; returns the
  instruction parts; `render_text` for text-only wire APIs.
- `Journal` and events — the journal API; pointer to the event
  catalog.
- `tools` — `Tool`, `Toolbox`, constructors.
- `clients` — registry functions, built-in clients, wrappers.
- `wire` — each helper, signature, behavior.
- `errors` — the `ai.errors` namespace: the interface, `RetrySafety`, each class.
- Standard library dependencies — `baml.schema.json_schema`,
  `baml.sap.parse`, `baml.json`, `baml.http`, `reflect`, `baml.env`;
  which layer uses each and where the `wire` helpers sit above them.

### 02_events.md

- The catalog — one table: event, fields, producer, when appended.
- Ordering rules — what a committed turn batch contains; ordering
  within a batch.
- Rendering rules — which events lower into model input and which are
  journal-only.

### 03_errors.md

- The catalog — one table: class, fields, retry safety, thrown by,
  and the condition that produces it.
- The classification table — HTTP status to failure class.
- Retry safety — how `RetrySafety` and the wrapper defaults interact.
- Throwing your own — errors are plain classes; implement `Failure`
  with a `retry_safety()` answer to participate in retry
  classification.
- The unknown channel — `baml.errors.UnknownError` wraps untyped
  throws; the `ai.Failure | baml.errors.UnknownError` convention on
  fallible signatures.

## Appendix

### 01_comparisons.md

- pi — providers as descriptors over reusable wire APIs; per-block
  signatures. Adopted: local state is sufficient for resume, replay
  data is narrow rather than the HTTP envelope, continuation decisions
  belong to the wire adapter, checkpoints do not make clients
  stateful, one entry point with option slots. Avoided: one overloaded
  session id serving cache, affinity, and continuation.
- Pydantic AI — message arrays as state; why the journal instead.
- OpenAI Agents SDK.
- BEPv4 (`begin`/`step`/`submit`) — what carried over (failure
  taxonomy, runner-owned loop, capability-by-interface) and what was
  dropped (mutable conversations, provider-generic `step<T>`,
  pre-rendered task prompts).
- The sessions draft (`_plan/`) — what this BEP keeps (journal,
  content-block direction), what it defers (sessions, policies,
  steering), and the client-boundary changes
  (`render`/`invoke`/`ingest` collapsed to `invoke`).

### 02_alternatives_considered.md

- One public `invoke` versus three public phases.
- No `runner:` field in the function block — the block is a static
  template; a runner is application infrastructure; the explicit form
  is `my_runner.run(f@spec(...))`.
- `$` parameters are runner fields — one namespace, one desugar.
- One client override — `$client` maps to the runner's `client` field;
  `with_client` on the spec rejected as a second way to say the same
  thing.
- Specs are immutable — getters only; rebinding methods rejected.
- Tool errors report by default — raising is opt-in per tool or per
  run; a thrown tool error is journaled before it propagates.
- Materials, not renderings — the prompt template and `output_type` in
  the turn input versus pre-rendered strings.
- Structured content blocks versus string plus `raw_json`.
- Journaled state versus provider-owned conversations.
- The runner parses, the client normalizes — where typed output
  recovery lives.
- Registry resolution versus a public service-descriptor interface.
- SAP-first structured output — `output_mode` ships with the single
  value `Sap`; `Native` and `Strict` are phase 2 values of the same
  field.
- Wrapper clients for reliability versus runner-level retry.
- Optional capability interfaces versus a capabilities struct.
- Deterministic synthesized call ids versus counters in state.
- The prompt is the instructions; no transcript placeholder — the
  marker's position was wire fiction; trailing content returns as
  injected messages with sessions.
- Prompt-mode tools are a wrapper client, not a client mode — v4's
  `tool_mode` flag rejected; one composable `PromptTools`
  implementation in phase 2.
- The `Runner` interface requires no fields — no option is universal;
  shared options come from embedding an `Agent`.
- No sessions in this BEP — what forced the cut, what re-entry
  requires.
- Adjustments forced by the reference implementation — the `client`
  keyword and `default_client`; nullable per-tool `on_error`; Gemini's
  every-turn instructions rule; float widening in argument validation;
  events carrying serialized JSON pending a `json`-typed event design.
- The journal records repair attempts — `Journal.with` removed;
  `append_all` is the write; rendering filters, the record does not.

### 03_future_phases.md

- The two invariants — structured assistant content; the journal alone
  renders a complete request; why every later phase is additive if
  they hold.
- Phase 2 — fidelity and streaming: replay capsules and wire domains;
  native structured-output modes; `StreamingClient`; the `PromptTools`
  wrapper; media-output binding.
- Phase 3 — continuations: journaled response-chain checkpoints;
  context policy on the turn input; delta rendering; classified
  fallback.
- Phase 4 — remote state and long-running work: remote conversations
  as an explicit storage mode; background and batch capability
  interfaces.
- Sessions — a runner with a durable journal; what from this BEP they
  reuse unchanged.
