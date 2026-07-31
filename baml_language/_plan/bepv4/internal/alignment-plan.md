# BEPv4 alignment: execution plan

Internal working plan. Enumerates everything required to bring the stdlib,
the scenario corpus, and the BEP pages into full agreement with the decided
design (session-redesign.md D1–D12 plus review outcomes). Each phase ends
with a verification gate; nothing advances past a red gate.

Companion docs: [session-redesign.md](../session-redesign.md) (decisions),
[pages/agent-sessions.md](../pages/agent-sessions.md) and
[pages/errors-and-error-handling.md](../pages/errors-and-error-handling.md)
(already-rewritten user pages).

## Already landed (baseline — do not re-plan)

- `ToolResult = ToolOk | ToolError` union + `result_id/result_is_error/
  result_payload`; all adapters and fakes migrated; wire bytes unchanged.
- `ai.run.AgentSession<T>`: `of / send / complete / resume /
  submit_tool_results / fork / save / restore`; self-advancing (D7); no
  `ask`/`answer`/`after` (D8, D10).
- `task.complete(runner?)`; `ai.IncompleteRun` (lossless); public
  `require_done` deleted; corpus test helper `done_or_fail` in
  `00_shared/models.baml`.
- `ai.Failure` = `effects()` only; `ai.retry(provider, attempts,
  retry_if?)` (D12).
- Memory agent: `memory` namespace, plain-function tools, sessions,
  self-advancing turns.
- Pages rewritten: agent-sessions, errors-and-error-handling; routing page
  retry-gate section updated; meta.json + README indexed.
- Verified at baseline: 174-file check, 243/243 deterministic, live integ
  (OpenAI/Anthropic/Google AI/Vertex tool loops, memory-agent
  continuation), live REPL smokes.
- Linear: B-1114 (backtick trimming), B-1116 (union interface dispatch
  panic — blocks union-method sugar; all workarounds use match helpers).

---

## Stdlib gaps: current stdlib vs decided design

The mirror of the BEP audit — what the *code* is missing or still carries
from the pre-session design. Each row maps to the phase that closes it.

| # | Gap (current stdlib state) | Designed state | Phase |
| --- | --- | --- | --- |
| S1 | `Agent<T>` still has `conversation` field/param — the deprecated continuation channel; sessions inject via runner-spread (silently drops a caller-supplied conversation) | Runner is pure policy; sessions are the only continuation entry (D1) | 1 |
| S2 | No `_AgentStart`; append/submit happen in session methods *outside* the run boundary, before invariants | Total internal start union; mutation inside the boundary (D2) | 1 |
| S3 | No `Failed` outcome — a provider failure mid-continuation throws, leaving the session at an undefined half-advanced state (append committed, no outcome observed); retry-by-resend double-appends | Failures after the first commit return `Failed { cause, conversation, steps_taken, usage }`; throw ⇒ provably unchanged (D11) | 1 |
| S4 | No single-flight guard — concurrent continuations on one session race on `self.conversation` | `SessionBusy` typed error (D11) | 0.1 |
| S5 | Stop outcomes carry `steps_taken` but no `usage`; `Done` carries neither — cost budgets cannot continue across resume; memory-agent Observer hand-counts steps | `{steps_taken, usage}` on all outcomes; budget = total-cap against carried accounting | 0.5 / 1 |
| S6 | `AgentSessionToken.task_identity` = name only (package dropped — cross-package collision); no structural contract check (same-`T` task swap undetected) | package+name now; contract fingerprint (tools/schemas/output/provider) later | 0.2 / 3 |
| S7 | `IncompleteRun` has no `conversation()`/`steps_taken()` accessors — union field access blocked (B-1116) forces a 3-arm match at every catch | Accessor methods on the class | 0.3 |
| S8 | `fork()` = `baml.deep_copy` of opaque sealed state — an implementation assumption, not a protocol guarantee | Fork = `restore(save(conv))` behind the existing `ResumableAgentProvider` gate; `Unsupported` otherwise | 0.4 |
| S9 | Conversation protocol has no `pending_calls()` — no runtime typestate guards, no uniform correlation check, no derived `phase()`; wrong-verb calls fail deep inside adapters with per-adapter wording | `pending_calls() -> ToolCall[]?` (null = provider doesn't report) + guards + strict correlation + `phase()` (D6/D9) | 2 |
| S10 | No `export()`/`import()` on sessions — the exact↔portable pole crossing is scattered (`conversation.messages()`, `import_messages`) and invisible | Two named methods; `ConversationFidelity` surfaced (D5) | 2 |
| S11 | `send`/`complete` take one `Message` — a drained REPL queue costs one model turn per message | Accept `Message | Message[]` | 2 |
| S12 | `EffectCommitted` overlaps `Failed` for in-loop commit failures | Narrow to non-conversation operations (jobs/batches) | 1 (decide) |
| S13 | Adapter atomicity ("mutate conversation only after wire success") is a routing-page footnote, untested | Stated `AgentProvider` rule + per-adapter contract test | 1 |
| S14 | `run_id`/registry/`state` reset silently on resume-with-fresh-runner | Documented reuse-the-runner recipe (in-process) + accounting continuation; full `AgentCheckpoint` decided in Phase 1 | 1 |

**D11 refinement (recorded here, supersedes the staged-copy sketch):**
*the append IS the first commit of a `_NextStart`.* No staged copy is
needed: if the append itself fails, adapters guarantee the conversation is
unchanged (S13) and the method throws with the session untouched; once the
append succeeds, every later failure returns `Failed` whose conversation
includes the appended message — so `resume()` retries the model step
without re-appending. For fresh runs, the pre-progress window (through the
first provider step) still *throws*, preserving today's catch-`RateLimited`
patterns and step-level `ai.retry` semantics; `Failed` begins once progress
exists (a completed step or submitted tool batch). `complete()` over a
`Failed` outcome throws the carried `cause` (open question 3: decided) —
`IncompleteRun.outcome` stays three-variant.

---

## Phase status

- Phase 0 — **complete** (guard, token identity, accessors, capability
  fork + fake save/restore, accounting on all outcomes).
- Phase 1 — **complete** (D1+D2+D11: `_AgentStart`, `Failed`, no
  `Agent.conversation`, `adopt`, corpus migrated, atomicity rule stated,
  transactional tests green). Follow-up: wiremock-level atomicity tests
  per real adapter.
- Phase 2 — **complete** (`pending_calls()` nullable protocol + fakes +
  client adapter; guards; exact correlation; `phase()`; export/import;
  batch send). Follow-up: real-adapter `pending_calls()` overrides.
- Phase 3 — **complete** (contract fingerprint in task + token + restore).
- Phase 4 — **complete** (provider_for, AgentTurn/SupervisedTurn,
  EditReport partial outcomes, store determinism + caps + recovery,
  ToolFault typed tool failures, fork-poles cross-references).
- Phase 5 — **complete**, verified by a final fresh-eyes audit of all 17
  doc files against the finished stdlib. Audit outcome: banned-API scan
  clean, session surface exact, all 17 runnable commands resolve; six
  residual findings all fixed (mermaid 4-variant enumeration, "two-method
  interface" wording, `root.`→`user.` task-identity output, ungrounded
  PlanTrip/Itinerary example domain → ResolveTicket/Resolution,
  `provider: "vendor/model"` string shorthand → grounded expressions in
  six pages, H1/meta title alignment). Accepted nit, recorded: three pages
  redefine `ResolveTicketWithTools` with page-local tool rosters — each
  self-consistent, left as teaching redefinitions.
  Done earlier in the phase: agent-sessions + errors pages (inline, new
  five-outcome surface + delivered D11 invariant); reference set via
  checklist agent — routing (retry_if example, effects-only gate,
  import-based switch), testing (fake tool providers subsection, five-variant
  unions), harness (`_instance_id` literal fix), why-baml (AgentSession
  item), structured-outputs (result_payload note), README (five variants,
  completion verbs, namespace tree, guides links), GROUNDING (normative
  rows for sessions/completion verbs/ToolResult union/retry_if). Remaining:
  continuation-pages agent (conversations-and-resuming restructure,
  approvals, tasks-runners, agents-and-tools) + final checker audit.
- Post-phase hygiene: every cross-cutting private helper moved into
  `ai.internal` (`_done`, `_outcome_conversation`,
  `_require_exact_correlation`, and the start union as
  `ai.internal.FreshStart/ResumeStart/NextStart/AnswerStart/AgentStart` —
  classes drop the underscore because the namespace is the privacy marker,
  matching `ai.internal.ClientProvider`). `ai` and `ai.run` public surfaces
  carry no loose helpers.

## Phase 0 — Immediate correctness fixes (independent, ship first)

Small, additive, each independently testable.

| # | Item | Detail |
| --- | --- | --- |
| 0.1 | Single-flight guard | `busy: bool` on `AgentSession`; every continuation method sets/clears it and throws a typed `ai.run.SessionBusy` if already set. Rationale: sessions are shared mutable objects; the memory agent runs turns inside `spawn`. |
| 0.2 | Token identity bug | `AgentSessionToken.task_identity` stores only `identity.name` — two same-named tasks in different packages collide. Store `package + "." + name`; update `restore` comparison and the mismatch test. |
| 0.3 | `IncompleteRun` accessors | `conversation()` and `steps_taken()` methods on the class (three-arm match inside — union field access is blocked by B-1116). Document: act on the error before continuing the session; the conversation is a live reference, not a snapshot. |
| 0.4 | Fork via save/restore | Replace `baml.deep_copy` in `fork()` with `restore_conversation(save_conversation(conv))` — a provider-blessed copy behind the existing `ResumableAgentProvider` gate; throws `Unsupported` otherwise. Keeps "providers untouched" true. Update fork tests (fake providers must implement save/restore, or tests use the paused-openai fixture). |
| 0.5 | Accounting on outcomes | Add `usage: ai.Usage` to `BudgetReached`, `Handoff`, `Interrupted`; add `steps_taken: int` and `usage` to `Done<T>`. Update loop construction sites + fakes + any outcome literals in corpus. Enables total-budget semantics and cost continuation across resume; numbers are serializable so they can later join the session token. Removes the Observer step-counting workaround in the memory agent. |

**Gate 0:** rebuild; both corpora check; deterministic suite; memory-agent
observer simplification verified; live memory-agent continuation.

---

## Phase 1 — Core refactor: D1 + D2 + D11 (one unit)

The loop rewrite. These three land together because they touch the same
function.

1. **Runner purity (D1).** Remove `conversation` from `Agent<T>` fields and
   `Agent.new`. `task.run(runner)` is always fresh.
2. **Internal `_AgentStart` (D2).** `_FreshStart | _ResumeStart |
   _NextStart | _AnswerStart`, private to `ai.run`. Provider selection and
   conversation acquisition become total matches; append and submit move
   inside the boundary after invariant checks. Session methods call the
   internal entry — the runner-spread hack is deleted.
3. **Transactional semantics (D11).** The invariant: *a continuation
   returns an outcome (session at exactly that committed state) or throws
   (session provably unchanged).*
   - New fifth outcome `ai.Failed { cause: ai.Failure, conversation,
     steps_taken, usage }`. Any failure after the first commit of a run is
     caught at the loop's existing checkpoint structure and returned as
     `Failed`. Distinct from `Interrupted` (user-driven vs involuntary —
     the REPL renders them differently).
   - Staged entry: `_NextStart` appends on a staged copy adopted at first
     commit; a pre-commit throw leaves the session untouched.
   - Adapter atomicity contract: providers mutate conversation state only
     after wire success. Promote from the routing-page footnote to a stated
     `AgentProvider` rule; add one deterministic contract test per adapter
     (openai responses/prompt, anthropic native/prompt, google ai/vertex,
     claude_code, fakes).
   - Retry story: transient `Failed` → `session.resume()`. Document as the
     session-level counterpart of step-level `ai.retry`.
4. **Run-state continuation (review pt. 2).** With the loop restructured:
   budget interpreted against carried `steps_taken`/`usage` (total-cap
   semantics; fresh allotment = explicit reset); document the
   reuse-the-runner recipe for in-process registry/`state`/`run_id`
   continuity; the full `AgentCheckpoint` object is the D1+D2 shape of
   session state if implementation friction is low, otherwise deferred
   with rationale recorded.

**Corpus migration for Phase 1** (union widening to five + `conversation =`
removal):

- `Agent.new(conversation = ...)` sites → session calls: `cancellation.baml`
  (1), `handoffs_and_budgets.baml` (1), `multiple_and_parallel_tools.baml`
  (1), `save_and_resume.baml` tests (3), `memory_agent.baml` tests (1).
- Explicit 4-union type annotations gain `Failed` (session method
  signatures in stdlib; `_finish_turn` and `done_or_fail` in corpus; any
  test matching exhaustively without `_ =>`).
- Memory agent `_finish_turn` gains a `Failed` arm (render distinctly from
  `Interrupted`).

**Gate 1:** rebuild; both corpora; deterministic suite; live: all four
provider native-tool suites + memory-agent continuation + a live REPL smoke
including ESC interrupt; new adapter atomicity tests green.

---

## Phase 2 — State guards + poles: D5 + D6 + D9 (one additive unit)

1. **`pending_calls()` on the conversation protocol (D6 prerequisite).**
   Default implementation + per-adapter overrides (each adapter already
   tracks pending state internally; expose it). Fakes included.
2. **Runtime typestate (D9).** All three continuation verbs validate:
   `send`/`complete` reject pending calls ("unanswered handoff; call
   submit_tool_results"); `submit_tool_results` rejects none-pending;
   `resume` rejects a completed turn. Error messages name the correct verb.
   Steering stays legal: `send` after `BudgetReached`/`Interrupted` is a
   feature, not an error.
3. **Strict correlation (D6).** `submit_tool_results` validates results ↔
   pending call IDs exactly (each answered once, no extras) before submit,
   with one uniform typed failure replacing per-adapter wording.
4. **Derived phase accessor.** `session.phase()` computed from the
   conversation (never stored): `ReadyForMessage | PausedTurn |
   WaitingForTool { call }` — the pending call surfaces so applications
   need not retain the `Handoff` outcome.
5. **The poles (D5).** `session.export() -> ai.Messages` and
   `AgentSession.import(task, messages)` via `ConversationImportProvider`;
   `ConversationFidelity` surfaced on import. Movement between exact and
   portable is always these two visible calls.
6. **Batch send.** `send`/`complete` accept `Message[]` (or a `messages`
   overload) so a drained REPL queue becomes one turn.

**Gate 2:** deterministic suite + new guard/correlation/phase tests; live
handoff suite; scenario `06_agent_session` extended to demo `phase()` and
`export`/`import`.

---

## Phase 3 — Structural contract fingerprint (review pt. 3)

- `task.contract_fingerprint()`: digest over package+name, output type,
  provider protocol name, tool names + input schemas + handoff flags.
  Prompt digest excluded (renders per-provider; not re-sent on
  continuation).
- `AgentSessionToken` gains `contract_fingerprint`; `restore` verifies it
  and the docs say "structurally compatible task" (handler identity is
  unprovable — same reason tasks don't serialize).
- Keep 0.2's identity check as the fast path + clearer error; fingerprint
  as the deep check.

**Gate 3:** save/restore tests extended (same-name-different-tools task is
refused); sessions page save/restore section updated.

---

## Phase 4 — Scenario backlog (accepted review items not yet commissioned)

From the memory-agent review (priority order previously agreed):

1. Centralize provider selection: `_provider_for(backend, max_output_tokens)`
   collapsing the two backend matches.
2. `TurnResult`/`SupervisedTurn` split (`queued` only exists under
   supervision) + non-nullable `Session.task` restructure.
3. `apply_edits` partial outcomes: per-edit catch, applied/rejected lists;
   REPL reports both.
4. Store determinism: sort by name + tie-break, token/keyword dedup,
   injection caps, per-file parse recovery, clipped (not dropped) bodies in
   `index()`.
5. Typed tool failures: memory-agent tools throw a scenario failure
   implementing `ai.Failure` (guidance text preserved in the message);
   `_style_result` drops the `starts_with("ERROR")` fallback.
6. Fork scenarios as the two poles: keep `03_fork_a_conversation`
   (portable) and add a session-fork counterpart, cross-referenced from the
   sessions page fork section.

**Gate 4:** deterministic suite; live memory-agent REPL drive (multi-turn +
recall + ESC).

---

## Phase 5 — BEP accuracy crosscheck and fixes

Method: two read-only audit passes over every page + README + GROUNDING
against the current stdlib surface (audits running; findings to be recorded
below). Fix rule: every code snippet must be valid against `baml describe`
of the current stdlib; every table row checked against source; every page
that teaches continuation via `Agent.new(conversation = ...)` is rewritten
to sessions when Phase 1 lands (until then, annotate as legacy).

Known-stale candidates to verify (pre-audit expectations):

- `agents-and-tools.md` — outcome union arity; `Handoff` continuation
  example (manual `provider.submit` + `Agent.new(conversation=)` → session
  `submit_tool_results`); `throws never` on example tools (harmless but
  no longer required style).
- `approvals-limits-and-handoffs.md` — handoff resumption flow; outcome
  tables; `ToolResult` construction.
- `conversations-and-resuming.md` — save/resume flow predates
  `AgentSession.save/restore`; should present sessions as primary and raw
  `ConversationToken` as the provider layer beneath.
- `tasks-runners-and-results.md` — direct-call semantics vs `task.complete`;
  any `require_done`-era unwrap idiom.
- `testing-and-observability.md` — fake providers' `submitted_results`
  asserts if shown with `is_error`.
- `README.md` / `GROUNDING.md` — surface lists, error-vocabulary claims
  (`is_transient`), scenario inventory (new `06_agent_session`).
- Historical design docs (`error-model-plan.md`, `error-redesign.md`) are
  history — annotate with a one-line header pointing at session-redesign.md
  rather than editing content.

### Audit findings

**Pass A (pages 0–6) — complete.** Checklist:

`agents-and-tools.md` — minor:
- [ ] "When you need the exact outcome" (~L186): "submit a `ToolResult` for
  `handoff.call` before resuming its conversation" → `ToolOk.of`/
  `ToolError.of` + `session.submit_tool_results([...])`.
- [ ] Add `task.complete(runner?)` as the documented middle ground next to
  "use the direct call when incomplete outcomes are exceptional" (~L183).
- Everything else verified current (four-variant union, plain-function
  tools, bound methods, `tools = []` semantics).

`why-baml.md` — accurate; one enrichment:
- [ ] "Where BAML is actually different" + comparison table never mention
  `AgentSession` — add the continuation story vs Vercel/Pydantic session
  APIs.

`tasks-runners-and-results.md`:
- [ ] L146: "non-transient `ai.Failure`" — dead vocabulary post-D12; only
  `Effects.None` remains.
- [ ] L158–163: correlated-result flow → `ToolOk.of`/`ToolError.of` +
  `session.submit_tool_results`, not rebuilding an Agent around a raw
  conversation.
- [ ] L234 "internal unwrapping helpers in scenario code" →
  `task.complete`/`session.complete` + `ai.IncompleteRun`.
- [ ] "The two entry points" (L8–34) + "Result map" (L286–295): add
  `task.complete -> T throws IncompleteRun` as the third first-class entry
  point; add `AgentSession` to the result surface.
- [ ] L141: resuming `Interrupted` → teach `AgentSession.of(...).resume()`.

`dynamic-tools-and-mcp.md` — fully accurate, no changes.

`structured-outputs-and-tool-calling.md` — fully accurate; optional
enrichment:
- [ ] Mention `ai.tools.result_payload` as the shared `{"error": message}`
  adapter payload.

`approvals-limits-and-handoffs.md`:
- [ ] L163: "non-transient with `Effects.None`" → drop transient framing.
- [ ] L184: `ai.tools.ToolResult.error(call, ...)` →
  `ai.tools.ToolError.of(call, ...)`.
- [ ] L176–194: manual `provider.submit` + `Agent.new(conversation =
  continued)` batch-rejection flow → session form.
- [ ] L261: `ToolResult.ok(handoff.call, ...)` → `ToolOk.of(...)`.
- [ ] L264–268: handoff completion via `Agent.new(conversation = ...)` →
  `session.submit_tool_results(results, runner?)` (note: manual version
  also silently re-renders the task).
- [ ] L272–274 prose: `ToolResult.ok/error` naming → `ToolOk.of`/
  `ToolError.of` + result helpers.

`conversations-and-resuming.md` — **restructure, worst offender**; frame is
pre-session. Keep the accurate low-level content (append rules,
`ConversationAppendProvider`, `ConversationToken`/`ConversationFidelity`,
retry/fallback append delegation) as the "provider layer beneath sessions":
- [ ] Premise (L3–4) + utilities table row (L14): `Agent.new(conversation =
  ...)` → sessions as primary; raw conversation passing as the layer
  beneath.
- [ ] Example (L39–59): 4-arm conversation extraction + `Agent.new(...)` →
  `AgentSession.of(task, outcome)` + `session.resume(runner)`.
- [ ] L46–50: Handoff arm throwing `Unsupported` → handoff is a legitimate
  session state; `submit_tool_results` is its continuation.
- [ ] "Start a fresh user turn" (L101–121): manual `append_message` +
  `Agent.new(conversation = continued)` → `session.send(...)` /
  `session.complete(...)`.
- [ ] "Interrupt and resume" (L197–209): → `AgentSession.of(...).resume(
  runner = Agent.new(cancel = fresh_token))`.
- [ ] "Save it for another process" (L236–247): `save_conversation`/
  `restore_conversation`/`Agent.new(conversation = restored)` →
  `session.save()`/`AgentSession.restore(task, token)` +
  `AgentSessionToken`/`SessionMismatch`; `ConversationToken` presented as
  the conversation half.
- [ ] "Move to another provider" (L288–295): imported conversation enters
  a session; note explicitly this is the one legitimately low-level spot
  until D5 `import` lands.
- [ ] Operations table (L310–315): add session rows (`send`, `fork` — the
  only supported branching, since sessions advance in place).

**Pass B (pages 7–13 + README + GROUNDING) — complete.** Checklist:

Fully accurate, no changes: `streaming-media-and-transcription.md`,
`jobs-batches-and-caches.md`, `voice-and-live-sessions.md` (optional: name
`ToolOk.of`/`ToolError.of` as how apps build correlated results).

`routing-retry-and-fallback.md`:
- [ ] L112 "transient, `Effects.None` failure" and L116–118 "terminal
  failures" — transience/terminal are dead vocabulary; the fallback gate is
  `effects() == None` only.
- [ ] L56–59 decision tree says "classified as safe" — contradicts the
  page's own corrected prose; reword to the effects + `retry_if` gate.
- [ ] L41–44: show the `retry_if` parameter in a code example, not just
  prose.
- [ ] L162–168: provider-switch continuation via `Agent.new(conversation =
  imported.conversation)` → session form (note: import is the one
  legitimately low-level spot until D5 `import` lands).

`errors-and-error-handling.md` (already current; two snippet fixes):
- [x] L90: bind `let task = PlanTrip@task(request);` before use in the
  `IncompleteRun` example — fixed.
- [ ] "Tool failures never throw": show `ToolError.of(call, message)` and
  the result helpers explicitly (enrichment).

`testing-and-observability.md`:
- [ ] Document `FakeToolProvider`/`ScriptedToolProvider` and
  `submitted_results: ToolResult[][]` assertion guidance (match
  `ToolOk`/`ToolError` or use `result_is_error`; never an `is_error`
  field).

`harnesses-and-custom-extensions.md`:
- [ ] L111–126: `claude_code.ClaudeCodeCli { executable, harness_sessions }`
  literal omits the required `_instance_id: string` field and would not
  construct.

`agent-sessions.md` (best-aligned; one snippet fix):
- [x] L200: `baml.json.from_json<...>(baml.fs.read(...))` →
  `baml.json.from_string<...>` (`from_json` takes `json`, `fs.read`
  returns `string`) — fixed.

`README.md`:
- [ ] L42–44 + L69–70 + L205: outcome union listed as three variants —
  add `Interrupted` (and `IncompleteRun` to the name tree).
- [ ] L46–49: `ToolResult` correlated-result prose → union +
  `ToolOk.of`/`ToolError.of`.
- [ ] L69–70: direct-call lowering — name `task.complete()` +
  `ai.IncompleteRun` (no `require_done`).
- [ ] L163–167: `ai.retry` signature gains `retry_if`; "classified as safe
  to retry" → effects + caller predicate.
- [ ] L206 namespace tree: add `run.AgentSession`/`AgentSessionToken`/
  `SessionMismatch`, `run.Transcribe`/`TranscribeWithMeta`/`VoiceAgent`.
- [ ] Guides table: link the four unlinked pages (errors, approvals,
  dynamic-tools, why-baml).
- [ ] Add the completion verbs to the "Direct calls" section.

`GROUNDING.md`:
- [ ] "Normative names" L43: four-variant outcome union.
- [ ] Add normative rows: completion verbs + `IncompleteRun`;
  `AgentSession`/`AgentSessionToken`/`SessionMismatch`;
  `ToolResult = ToolOk | ToolError` with `ToolOk.of`/`ToolError.of` (so
  future audits fail on `is_error`/`ToolResult.ok`).

**Gate 5:** a final verification pass re-runs both audits' checklists;
every runnable-example command in every page executed once against the
corpus; meta.json page list matches `pages/`; memory file updated.

---

## Open questions (decide before or during Phase 1)

1. `EffectCommitted` — narrow to non-conversation operations (jobs,
   batches) once `Failed` exists, or retire. Leaning: narrow, with the
   errors page already wording it that way.
2. `AgentCheckpoint` as a public class vs loop-internal accounting in
   Phase 1 — decide on implementation friction; record either way in
   session-redesign.md.
3. `Failed` in `IncompleteRun.outcome`'s union? A `complete()` over a run
   that fails-with-progress: does it throw the cause, or `IncompleteRun`
   carrying `Failed`? Leaning: throw the cause directly (the caller asked
   for completion; a fault is a fault), with the session still advanced to
   the committed state. Must be decided with D11.

## Standing verification requirements (every phase)

- `cargo build --bin baml-cli` after any `baml_std` edit (builtins are
  embedded via include_str).
- `baml-cli check` on `baml_src_temp2` (174 files) *and* main `baml_src`.
- Full deterministic suite (`-x "*integ*"`), currently 243.
- Live tiers with real keys: `infisical run --env=test
  --project-config-dir /Users/aaron/projects/baml -- ...` — memory-agent
  continuation, four-provider native-tool suites, plus a piped live REPL
  drive for phases touching the loop or session semantics.
- No `ai.internal.*` or underscore helpers in scenario/doc examples.

## Post-plan sweep — hide flat-`ai` underscores, purge `ai.internal` from scenarios (2026-07-30)

Directive: no underscore helpers may appear in the toplevel `ai` namespace,
and scenario code must not call `ai.internal.*`.

Stdlib/compiler changes:

- Moved to `ai.internal` (new `ns_internal/bridges.baml`, registered in
  `baml_builtins2/src/lib.rs`): `_provider_from_client`, `_task_named`,
  `_task_named_from_registered_prompt`, `_prompt_recipe`,
  `_registered_prompt_recipe`, `_fallback_member`.
- Compiler emission updated in `baml_compiler2_ast/src/lower_expr_body.rs`
  (both `Expr::Path` constructions gained the `internal` segment);
  `phase3a.rs` expected TIR strings updated to match.
- New public API replacing scenario-level internal calls:
  - `ai.Done<T>.response() -> ResponseWithMetadata<T>` (replaces
    `ai.internal._response_from_done` at 35 scenario call sites).
  - `ai.output_fingerprint<T>()` for provider authors declaring a
    conversation's output type (replaces `_output_type_fingerprint` at 5
    scenario sites).
  - `ai.PromptRenderRecipe.of(template)` for provider packages re-rendering
    a transformed template (replaces `ai._prompt_recipe` in ns_anthropic,
    ns_openai, ns_google, ns_claude_code).
  - `ai.tools.as_inputs(tools)` — public Tool[] → ToolInput[] widening
    (was `ai.tools._tool_inputs_from_tools`; renamed at all call sites).
- Scenario-only rewrites: `_same_provider_instance(a, b)` →
  `a.name() != b.name()` in handoff guards; the custom-runner example in
  01/05 now matches the public five-outcome union instead of calling
  `_run_agent_to_response`.
- Cleanups surfaced by the sweep: reordered `AgentSession` catch arms so
  `baml.errors.Unsupported` precedes `ai.Failure` (Unsupported implements
  Failure, so the old order made the arm unreachable); dropped a dead
  `catch_all` in the memory-agent curator; narrowed `_FlakyProvider`
  `begin`/`submit` to `throws never`.
- Accepted two stale insta snapshots (`phase5::snapshot_baml_package_items`
  gained `authenticate_request` from this branch's custom-provider work;
  `stream_expansion` reflects the agent-path desugar).

Still deliberately private (same-namespace `_` convention in public
sub-namespaces, not `ai.internal`): `ai.harness._*`, `ai.messages._prompt_*`,
`ai.observe._publish_agent_event`, `ai.run._pump_voice_input` /
`_interrupt_after_sustained_user_speech` / `_transcription_options`,
`ai.testing._fake_*`, `ai.tools._coarse_schema` / `_schema_for_function`.
The two white-box calls to `_interrupt_after_sustained_user_speech` in
`realtime_call_center.baml` are unit tests of the barge-in helper itself,
kept under the same precedent as provider-internal white-box tests.

Verified: both corpora check clean (zero errors, zero warnings in
`baml_src_temp2`); deterministic suite 257/257; `cargo test -p baml_tests
compiler2_tir` 389/389; live subset 7/7 (guide-01 across OpenAI/Anthropic,
custom-runner pin, interface-runner e2e).
