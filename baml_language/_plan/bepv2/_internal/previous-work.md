# Previous Work

This BEP consolidates several prior efforts. The main document and its pages
present the design fresh and do not reference this lineage; this file is the
only place it is recorded.

Names in this file are historical and intentionally retain the spelling used
by the designs being summarized. Current names are in the
[API reference](../pages/specification/api-reference.md).

## The design exploration (`_plan/../llm-provider/ideas/`)

A 47-scenario corpus (single-turn text through durable workflows) written
against a capability-interface model: `Provider` as a bare marker,
`HttpProvider`/`Streaming`/`Tools`/`Realtime` owning their interactions,
combinators as forwarding classes, errors as per-capability interfaces with
classifier methods, and a `call_with` value+metadata sidecar. Its
`_gap-analysis.md` graded every scenario workable-but-never-clean and
identified the recurring fault lines: no home for server-owned state, unsafe
combinator re-drive, no static capability requirements, control outcomes
forced through `throws`.

### Workflow scenarios 43–47

The final five scenarios explored distinct pieces that the workflow page
must keep separate:

- **43, workflow graph:** ordinary typed BAML control flow is already a
  better authoring surface than a heterogeneous graph value; durable
  coordinates and checkpoint semantics were the missing layer.
- **44, suspend/resume:** provider-owned agent suspension can be an honest
  provider resource, but opaque provider snapshots do not suspend arbitrary
  application control flow.
- **45, durable workflow:** recording only provider calls is too narrow for
  database effects, timers, signals, and tool handlers; best-effort
  checkpoint reads/writes are unsafe as a production contract.
- **46, workflow observability:** workflow lifecycle events need durable
  cursors and attempt/step identity; they are not `Partial<T>` model streams,
  and a generic `Step[]` representation loses useful types.
- **47, workflow/agent nesting:** whole-agent-as-step and workflow-as-tool
  are useful adapters, but side-effecting tools need finer durable dispatch,
  and a fixed `I -> O` workflow cannot honestly implement universally generic
  `Generate<T>`.

The removed workflow draft captured those constraints in a fresh
executor-based design: ordinary BAML authoring, typed durable commands,
resource tokens, replay rather than VM snapshots, and explicit adapters
where the signatures are honest.

### How the executor model supersedes each 43–47 mechanism

The original corpus modeled durability *inside* the provider layer — as a
capability, a combinator, or a provider that is a workflow. Each
evaluation then discovered a structural wart it could not fix from that
position. The removed workflow draft is those evaluations taken seriously:

- **43's `step(ckpt, run_id, name, body)` library memoizer** → rejected in
  the removed workflow draft's alternatives with the reasons 43 itself hit: a library callback
  cannot atomically commit an effect with the journal write, park a
  `T`-returning call, or wake timers. 43's conclusion — "the model solves
  the node, the language solves concurrency, the app supplies only
  durability" — *is* the removed workflow draft's thesis; the executor is "the app supplies
  durability" made a real component. 43's typed-error checkpoint loss is
  not fixed but honestly downgraded (`FailureSnapshot` normalization).
- **44's `Suspendable` capability + opaque `Snapshot`** → replay-not-
  snapshots dissolves 44's residue wholesale: no snapshot to migrate, no
  `freeze` that can refuse, no lost `client` sugar (a workflow is not a
  provider), no papered-over harness suspend. 44's real insights survive
  transformed: suspension-is-not-an-error becomes "waiting is a run
  status, not part of `Output`", and provider-owned agent suspension
  remains a legitimate provider resource with a token. The `Waiting | T`
  signature infection 44 flirted with is demoted to a fallback
  alternative.
- **45's `Durable` = Cache-with-`StepCoord`** → 45 named the fatal flaw
  itself ("the record/replay boundary is at the wrong layer, and the model
  can't say so"; hand-threaded coordinates silently alias). The removed workflow draft
  answers point by point: journal canonical values *plus type
  fingerprints* (`VersionMismatch`/`Decode` typed failures), lexical
  site + dynamic-key coordinates with duplicate rejection,
  input-hash-as-identity forbidden normatively, and the provider-retry ×
  activity-retry × replay table replacing unchecked determinism
  convention.
- **46's `Steppable`/`Workflow implements HttpProvider`** → both warts 46
  found were consequences of workflow-as-provider, so the removed workflow draft removes the
  cause: the untyped `unknown` inter-step carry disappears because the
  pipeline is ordinary typed control flow (no `Step[]` graph to erase
  types), and the faked token multiplex disappears because durable
  structural events (cursor-based) are split from live telemetry —
  "a workflow event stream is not `Stream<Partial<T>, T>`" is 46's lesson
  stated as a rule. Honest cost: 46's single unified stream becomes a
  client-side join of two streams.
- **47's bidirectional nesting** → the removed workflow draft overrules the verdict the
  corpus liked: workflow-as-provider ("genuinely clean" per 47's eval) is
  rejected because its cleanliness was bought with `tool_args<T>` decoding
  and the `cast.checked<T>` leak 47's own evaluation flagged; the typed
  `as_tool(executor, timeout)` binding keeps the good part with the real
  `I`/`O`. Direction A survives as whole-agent-as-one-activity with 47's
  safety condition made explicit, and the later fine-grained protocol
  supplies the `run / step / agent turn / tool-call id` coordinate
  vocabulary 47 said was missing.

What the corpus approach retains over the removed workflow draft: incrementality — pure
library over shipped language features plus two small host seams,
shippable immediately. The removed workflow draft defuses this with Phase A (host-engine
workflows over generated BAML activities, zero new syntax), which the
corpus never articulated. Switching cost is near zero in practice: the
branch realization of 43–47 stubbed every durable primitive
(P8-blocked `Checkpoint`, `RunStore`, `StepLog`, `baml.flow`), while the
language shapes it did validate (suspend-as-sum-arm, `spawn`/`await`
fan-in, agent-as-a-step) remain valid inside activities.

Weakest inherited spot to watch: `spawn`/`await` inside coordinator
bodies. The removed workflow draft's source-order/explicit-key branch coordinates cover it
on paper, but replay-under-concurrency is exactly where 43 leaned hardest
and nothing in the existing test corpus exercises it; Phase B journal
validation should cover racing branches first.

## The branch implementation (`crates/baml_builtins2/baml_std/baml/ns_ai/` + `crates/baml_tests/baml_src/ns_ai_scenarios/`)

The design above, largely built: native BAML providers (OpenAI, Anthropic,
Gemini, OpenAI-compatible, Responses, realtime), capability negotiation via
runtime interface `match`, generated companions (`$stream`, `$with`,
`$run_tools`, `$live`, `$render_prompt`, `$parse`), a user-capability
registry (`//baml:llm_capability` + `//baml:llm_companion(suffix)` markers
synthesizing `Foo$suffix` companions), `ToolLoop` wrapper providers, data
handles (`Job<T> {id, owner}`, `Session {_id}`, `ChainHandle`), and
combinators gated by `Provider.is_effectful()`. Offline scenario fixtures
plus live integ testsets exist for most of the corpus.

## The request-seam draft (`_plan/bep/`, BEP-063 draft)

A redesign proposal introducing `LlmRequest<T>` as the single seam type:
every LLM function gets a generated `$request` companion; standard drivers
are free functions (`baml.ai.run`, `run_with_meta`, `submit_background`, ...)
that negotiate capabilities at runtime; stateful operations return owned
resource objects (`Job<T>`, `Session`, `LiveSession`) instead of loose
handles; the user-capability registry is deleted in favor of ordinary driver
functions over requests; retry/fallback become operation-aware
(`ReplayPolicy` + commit state) instead of provider-wide `is_effectful`.

## The design review session (2026-07-09, `_plan/bep/alternatives_cookbook.md`)

A per-scenario comparison of all of the above plus new proposals that this
BEP adopts:

- **task modifiers as dot-methods** (`Foo.stream(...)`, `Foo.background(...)`)
  instead of `$`-suffixed companions or free-function drivers as the user
  surface;
- **`tools:` as a task-level field** (task-owned roster) with provider-owned
  server-side tools remaining client configuration — two rosters, merged by
  the provider;
- **return-type honesty** analysis: why plain calls cannot carry budget/
  handoff outcomes (the return type is also the output schema), and the
  graceful-finish escape hatch;
- **the three-layer surface rule**: task methods when holding a task,
  capability methods when holding a concrete provider, `baml.ai.*` free
  functions only as the negotiation layer for existential `Provider` values;
- **capability-transparency analysis** of combinators: why the
  existential-inner wrapper is the only capability-preserving design without
  intersection/Self types, and why refinement (semantic capabilities +
  per-operation replay policy) beats replacement;
- the **task-first vs model-first** framing (BAML task declarations vs
  agent-object libraries) that opens this BEP;
- the **line-count ledger** quantifying where a request seam costs code and
  where it refunds it.

SDK naming evidence gathered in the same session: classic generators use
mode-first namespaces (`b.stream.MyFunc`); the new-compiler Python SDK maps
`$stream` → `foo_stream` and other `$x` → `foo__x`; the new TypeScript SDK
preserves `$` verbatim.
