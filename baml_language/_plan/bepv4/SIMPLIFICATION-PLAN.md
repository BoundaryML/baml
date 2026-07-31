# BEPv4 simplification plan (v2)

> **Status (2026-07-31): IMPLEMENTED** — D1–D11 landed in stdlib + corpus +
> docs; 298 offline tests pass (0 offline failures; the 78 remaining
> failures are keyless live tiers), live spot-checks green via infisical
> (stop_when, retry/backoff, history/compaction/fork/save-resume).
> Deliberately deferred: doc-snippet CI extraction (D11, needs fixture
> scaffolding); claude_code keeps its internal `_owner_instance_id` strings
> (private convention, ai.same_provider_instance covers the public need).
> Two compiler bugs found during implementation (union-narrowing on
> interface-returned media unions; untyped `json.path_or` truthiness) are
> logged in the final session report.

Responds to `AUDIT-FINDINGS.md` (F-numbers refer to it). v2 incorporates review feedback:
no `ai.Budget` (lambda + top-level config instead), no fidelity surface for now, tools
stay mutable mid-conversation, jargon renamed to plain words, no conformance suite (the
doc page is the deliverable), steering-role appends, reasoning events wired up, and the
docs keep stdlib-style provider spelling.

Guiding principles, in priority order:

1. **One way to do each thing.** Every duplicated surface is deleted or demoted to the
   single blessed path's implementation detail.
2. **Composition over configuration.** Wrappers (retry, fallback, delegating providers,
   custom runners) are first-class citizens of every invariant, not special cases.
3. **Honest signatures, plain words.** A signature that compiles must not be a lie, and a
   public name must say what it does — no `fingerprint`, no `correlation`. If a concept
   needs a paragraph to name, it belongs behind the runtime, not in the API.
4. **Delete before designing.** A broken feature with no user is removed, not repaired.

Out of scope (deferred, not rejected): everything streaming (F4, F16 — noting once that
this is the one deferral that gets more expensive with time, as the concrete
`baml.llm.Stream` class calcifies), outbound request capture (F21 second half — safe to
defer because it is purely additive, provided event classes stay extensible), fidelity
reporting on the session surface (stays at the provider layer for now), and
assistant-role injection/prefill. A dedicated agent declaration shape was considered and
dropped: functions already cover it — the prompt is the instructions, dynamic content
arrives via args on first render or as chat messages on `send` (see D2's `start`).

---

## D1. Delete `ai.Budget`; `max_steps` + `stop_when` on the Agent (F2)

**Principles 3 & 4.** `max_cost_usd` is a safety limit that silently does nothing (no
adapter reports cost) — worse than absent. And a `Budget` class for what is really "when
should the loop stop" is configuration where a lambda composes better.

```baml
ai.run.Agent<T>.new(
  max_steps = 32,                                  // top-level, was Budget.max_steps
  stop_when = (ctx) -> ctx.usage.output_tokens > 200_000,  // optional policy lambda
)
```

- Delete `ai.Budget` and `Usage.cost_usd`; delete the dead cost check in
  `ns_run/agent.baml`.
- `stop_when` is evaluated at the same committed boundaries the budget check used
  (before each model request), receiving the same context object `prepare_step` gets —
  steps taken, cumulative usage, conversation. One context type, two hooks, no new
  machinery. Token caps, wall-clock caps, "stop if the model is looping" — all
  expressible without us designing each knob.
- Outcome rename: `BudgetReached` → **`ai.Stopped { conversation, steps_taken, usage,
  reason }`** with reason `"max_steps"` or `"stop_when"`. (Keeping a type named after a
  deleted class would be worse than the rename ripple; the outcome union becomes
  `Done | Stopped | Handoff | Interrupted | Failed`.)
- `ai.observe.TokenPrice` stays for apps that want dollar accounting in `stop_when` /
  observers; that's the documented pattern, not a framework promise.
- Docs: rip the spend-cap promise out of `approvals-limits-and-handoffs.md`.

## D2. One session constructor: `AgentSession.from` (adopt/import overlap)

**Principle 1.** Four constructors (`of`, `adopt`, `import`, `restore`) become a surface
where each entry has a distinct source type and no overlap:

```baml
type SessionSource = ai.Conversation | ai.Messages

AgentSession<T>.from(task, source) -> AgentSession<T>
    throws ai.Failure | baml.errors.UnknownError | baml.errors.Unsupported
```

- `Conversation` arm → validate invariants, use exact state (today's `adopt`).
- `Messages` arm → reconstruct through the provider (today's `import`);
  `Unsupported` if the provider can't.
- No fidelity surface on the session for now. Callers who care about exactness of a
  messages-seeded session drop to the provider layer (`import_messages`) where the
  report already lives. Revisit if it earns its keep.
- If interface-union ambiguity ever bites, switch to a tagged `SessionSource`; start with
  the raw union.

| Entry | Source |
| --- | --- |
| `AgentSession<T>.start(task)` | nothing — `provider.begin(task)`, zero I/O |
| `AgentSession<T>.of(task, outcome)` | an Agent outcome (documented sugar over `from(task, outcome.conversation)`) |
| `AgentSession<T>.from(task, source)` | exact `Conversation` **or** portable `Messages` |
| `AgentSession<T>.restore(task, token)` | cross-process token |

- **Delete `adopt` and `import`** (they become `from`'s two internal arms).
- **`start` is new**: turn one and turn N become the same code path (`start(task)`, then
  `session.send(...)` forever), killing the dual first/later-turn structure the memory
  agent had to build (F20). For conversational use, the function's prompt *is* the
  instructions — inject docs/context through the function's args at first render, and
  send everything dynamic after that as chat messages. No new declaration shape needed;
  this is documented on `start`.
- **String-first send.** `send` (and `complete`) take
  `message: string | ai.Message | ai.Message[]` — a plain string desugars to
  `ai.ChatMessage.user(...)`, so the everyday call is `session.send("...")`. Not
  string-*only*: media parts (D10a) and multi-part turns need the structured form, and a
  separate `send_message` method would be the two-ways-to-do-one-thing pattern this plan
  deletes. The union is the single way, with a gradient from simple to rich.

## D3. `session.move_to(provider)` — provider switch in one call

**Principle 2.** Pure composition of D2, ~10 lines:

```baml
function move_to(self, provider: ai.Provider) -> AgentSession<T>
    throws ai.Failure | baml.errors.UnknownError | baml.errors.Unsupported {
    // = AgentSession<T>.from(self.task().with_provider(provider), self.export())
    //   + refuse when phase() is AwaitingToolResults (the destination never issued
    //     those call IDs) with an error naming submit_tool_results
}
```

- Non-destructive: returns a new session; the original stays valid on its old provider
  (free rollback). Consistent with `fork`, unlike `send`.
- Lossiness (provider-private reasoning blocks, response IDs) is stated in the docstring;
  the provider-layer report remains available for callers who need to inspect it.

## D4. Delete the second execution surface: `ai.sessions` + `ai.run.InSession` (F17)

**Principle 1.** README says "there is no separate value-only or single-turn runner" —
make it true. `InSession` bypasses the outcome union, has no budget/handoff semantics, and
its protocol hinges on underscore-private `_execute` only `ai.internal` can satisfy.

- Remove `ai.sessions` and `ai.run.InSession` from the public tree; rewrite the
  scenario-04 usages onto `AgentSession` (strictly nicer after D2).
- De-overload the word "session" in docs (`AgentSession` / `HarnessSession` / realtime)
  with one glossary paragraph; no new token types per surface.

## D5. One reliability stack, defaults that match the docs (F3, F14, F15)

**Principles 1 & 3.**

**a. Default replay policy.** With no `retry_if`, decline
`Refused`/`InvalidRequest`/`ParseFailed` — the stdlib may have the judgment its own docs
table prints. `retry_if` remains the override in both directions.

**b. Transport failures become replayable.** The step contract already guarantees a
failing `step` leaves the conversation unchanged — that contract, not the failure class,
is what makes step replay safe. Classify step-raised `NetworkFailure` as `Effects.None`
(which `http.baml`'s comment already claims). The fail-closed gate remains for
application-tool effects — that stays sacred.

**c. Backoff.**

```baml
ai.retry(provider, max_attempts, retry_if?, backoff = ai.Backoff.default())
// ai.Backoff { initial_ms: int, multiplier: float, max_ms: int }
```

`RateLimited.retry_after_ms`, when present, overrides the computed delay — the field
finally has a consumer.

**d. One owner.** The ClientProvider bridge ignores legacy `baml.llm`
retry/fallback/round-robin config when a `client:` function executes through the ai path,
with a check-time diagnostic pointing at `ai.retry`/`ai.fallback`. No stacked
15-requests-per-step, no un-gated inner loop. Round-robin is not ported until someone
asks.

## D6. Tools: one declaration, one override, mutable mid-conversation, no cross-run leaks (F10, F11, F12, F13)

**a. Two supply points, loud precedence.** Static declaration (`tools:` on the function)
and one runner override. `Agent.new(tools=)` and `Agent.new(tool_registry=)` become
mutually exclusive (construction error). The runner override **merges with** task-declared
tools by default; full replacement is explicit (`replace_tools = true`). This kills the
silent Handoff→Done flip: a task-declared handoff tool cannot disappear because a runner
added a search tool.

**b. Mid-conversation mutation stays; cross-run leakage goes.** Tools remain mutable at
both granularities:

- *Between turns:* mutate the `ToolRegistry` you hold; each `send`/`resume` reads the
  registry's current state at run start. Add a tool after turn 3, turn 4 has it.
- *Within a turn:* `prepare_step` returning a `StepPlan` roster remains the sanctioned
  per-step mutation point (MCP bootstrap, policy revocation mid-loop).

The only change: the Agent applies `StepPlan` rosters to a **run-local snapshot**, never
by calling `replace_all` on the caller's registry object. Framework-internal mid-loop
changes stop leaking into the caller's registry and contaminating the next run (F11).
Your mutations flow in; the loop's mutations don't flow out.

**c. Un-pin the tool throws channel.** Tool handlers may be ordinary fallible functions —
including LLM functions (agent-as-tool). The boundary catches everything and builds:

```baml
class ToolError {
  id: string,
  message: string,           // rendered user-facing, never debug-format
  cause: (ai.Failure | baml.errors.UnknownError)?,  // the typed original, preserved
}
```

`after_tool_call`/observers match on `cause` instead of string-parsing.

**d. Leave `ModelStep` alone** (F12). The audit's `T = ai.tools.ToolCalls` trap only
fires if someone declares a function whose *output type* is `ai.tools.ToolCalls` — not
how the API is used; real code returns its own types and lets the Agent run the tools.
Not worth a protocol change. Close the corner with a one-line construction error
(`Agent` rejects `Task<ai.tools.ToolCalls>` with a message saying why) and move on.

## D7. Honest mutation signatures (F5, F18)

**Principle 3.**

- `Conversation.append_message(s)` (and the `ConversationAppendProvider` methods beneath)
  return `null` — statement-shaped, since every implementation mutates the receiver.
  Docstring: "mutates in place; use `session.fork()` to branch." `MessageHistory.append`
  stays copy-on-write. Documented rule: **provider-owned state mutates and returns null;
  portable value types are copy-on-write.**
- `AgentSession` fields become `_task`/`_conversation`/`_busy` with read accessors
  (`task()`, `conversation()`). Literal construction now reads as deliberate internal
  access; the stdlib's own tests stop modeling the bypass.

## D8. Make the direct-call contract true (F6)

**Principle 3.**

- Lower `F(...)` through the same path as `task.complete()`, so a stop state throws
  `ai.IncompleteRun { outcome }` — resumable, conversation intact — instead of a lossy
  bare `UnknownError`.
- **`IncompleteRun` stops implementing `ai.Failure`.** A stop is control flow, not a
  fault; generic failure arms must not silently absorb it. It becomes its own term:
  `throws ai.IncompleteRun | ai.Failure | baml.errors.UnknownError`.

## D9. Provider authorship: internalize the jargon, publish plain helpers (F0, F7, F8, F9)

**Principles 2 & 3.**

**a. The output-type check goes fully internal.** What `output_type_fingerprint` actually
is: a tag recording *which output type a conversation was created for*, so a conversation
can't be resumed by a task expecting a different type. Legitimate invariant, terrible
name, and currently user-visible (worse: user-*constructed*, via `ns_internal`). Fix: the
runtime stamps the tag after `begin()`; the interface member and its lying "may return
null" docstring are removed from the public surface; no public name replaces it. The
rejection error does the explaining: `conversation was created for output type
Resolution; this task expects Invoice`. (`ai.output_fingerprint<T>()` can retire with
it — hand-rolled runners re-enter through sessions, which stamp it.)

**b. `render_shorthand`: documented `vendor/model` grammar; malformed values raise
`ai.InvalidRequest`, never a panic.**

**c. Identity and delegation as first-class concepts.**

```baml
ai.same_provider_instance(a: ai.Provider, b: ai.Provider) -> bool

interface Provider {
  function delegate(self) -> ai.Provider? throws never { null }  // default: leaf
}
```

Ownership checks walk the delegation chain, so a thin forwarding wrapper is legal at any
nesting depth; `_instance_id` hand-hacks leave the docs.

**d. Public helpers, plain names** (promoted from `ns_internal`, same implementations):

| Was (internal) | Becomes | Plain meaning |
| --- | --- | --- |
| `_add_usage` | `usage.add(other)` / `ai.Usage.zero()` | sum two usage records |
| `_validate_tool_calls` | `ai.tools.check_calls(calls, roster, limit)` | the batch only names known tools and fits the limit |
| `_require_exact_correlation` | `ai.tools.check_results(calls, results)` | every pending call gets exactly one result |
| `_classify_http` | `ai.failures.classify_http(provider, status, body)` | shared status→failure table, so wrappers don't drift |
| `Task._render` | `task.recipe()` | the render recipe adapters need |

**e. Write `pages/implement-a-provider.md`.** A complete minimal adapter (the audit's
verified `AcmeProvider` is the draft) walking every obligation in order: `begin` renders,
`step` commits only after wire success, `submit` checks results (via `check_results`),
`pending_calls` tells the truth, `messages()` includes assistant turns and tool results.
No conformance suite for now — the page is the contract. The known adapter gaps it
documents (inert `pending_calls` on OpenAI/Anthropic/Google, fakes dropping assistant
history — F9) get fixed as ordinary bugs alongside it.

## D10. Conversational readback and steering (F19, F20, F21 — reduced scope)

**a. Append accepts media parts in user messages.** Every wire API supports them in
continuation requests; the text-only restriction is ours alone.

**b. A portable steering message.** The real-world need behind "non-user roles": inject
instructions mid-conversation (compaction summaries, context, policy changes). Wire
reality: OpenAI Responses accepts `system`/`developer` items mid-conversation natively;
Anthropic and Gemini have no mid-conversation system role (system is a top-level/
separate param) — production harnesses there inject tagged user-role content. So:

```baml
session.steer("The user switched to the billing account; prior order IDs are stale.")
```

a session method taking a plain string (steering is almost always text; the
`ai.ChatMessage.steer(...)` constructor exists underneath for append-path composition,
but application code uses the method). Each adapter maps it honestly — OpenAI → native
developer item; Anthropic/Gemini → user-role content wrapped in a documented
`<steering>` tag. No pretending a vendor has a capability it lacks. Assistant-role
injection (prefill/few-shot) stays deferred.

**c. Wire up assistant text and reasoning — the vendors do emit it.** `ModelStep` gains
`assistant_text: string?` and `reasoning_text: string?`; the Agent emits
`AssistantTextEvent` / `ReasoningEvent` when present. Adapter sources: Anthropic —
thinking text when extended thinking is on (signed blocks stay in conversation state, the
display text goes in the event); OpenAI — reasoning summaries when requested; Gemini —
thought summaries via `includeThoughts`. The memory agent's `summary`-argument smuggling
hack retires.

**d. Turn-structured transcript (full version, not just `last_turn`).** Every
conversational app renders a transcript; giving only the latest turn would still force
apps to accumulate their own shadow copy — the exact pattern F20 condemned. So:

```baml
class Turn {
  messages: ai.Messages,        // what was committed this turn
  tool_calls: ai.tools.ToolCall[],
  tool_results: ai.tools.ToolResult[],
  assistant_text: string?,
  reasoning_text: string?,
  usage: ai.Usage,              // this turn's delta
  metadata: ai.ResponseMetadata?,
}

session.turns() -> Turn[]       // whole transcript
session.last_turn() -> Turn     // sugar: turns().last()
```

Design constraint: no second log. Turn boundaries are marked in the portable history so
`turns()` is *derived* from `conversation.messages()` — no duplicated state, `save()`
tokens unaffected.

**e. Telemetry floor:** `emitted_at` timestamp on the `AgentEvent` interface, plus
`StepFinishedEvent { index: int, metadata: ai.ResponseMetadata, usage_delta: ai.Usage }`
so intermediate request IDs survive and spans are constructible. `UsageEvent` stays
cumulative; the delta lives on the step event.

## D11. Coherence gates (F1)

- CI gate: compile an empty user project + run one test against the embedded stdlib on
  every build (would have caught the `bridges.baml` breakage instantly).
- `baml-cli describe` and diagnostics render package-qualified public names
  (`ai.AgentProvider`, never `root....`), so pasted output is valid code.
- Provider spelling: **docs deliberately keep stdlib-style `openai.X` / `google.X`** —
  the BEP presents providers as if already migrated into the stdlib; no `root.` noise in
  any page. The corpus keeps `root.openai.X` until the migration lands. Doc-snippet CI
  extraction rewrites the provider prefix mechanically (`openai.` → `root.openai.`) so
  examples still compile-check without polluting the prose.

---

## Sequencing

| Phase | Contents | Character |
| --- | --- | --- |
| 1 | D1 (Budget → max_steps/stop_when), D4 (delete ai.sessions), D8 (direct-call contract), D6d (ToolCalls construction error), D7 (mutation signatures) | deletions + contract truth |
| 2 | D2 (`from`/`start`), D3 (`move_to`), D10a/b/d (media append, steer, turns()) | the session surface |
| 3 | D5 (reliability) | one stack, right defaults, backoff |
| 4 | D6a–c (tools), D10c/e (reasoning events, telemetry floor) | tools + observability |
| 5 | D9 (internalize jargon, publish plain helpers, implement-a-provider page), D11 (gates) | authorship + enforcement |

Net public-surface change from phases 1–2: removed — `ai.Budget`, `Usage.cost_usd`,
`ai.sessions`, `ai.run.InSession`, `adopt`, `import`, `output_type_fingerprint` (public
member), `IncompleteRun`-as-Failure; added — `stop_when`, `max_steps` (relocated),
`from`, `start`, `move_to`, `ChatMessage.steer`, `last_turn`, `ai.Stopped` (rename).
