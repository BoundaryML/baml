# Agent sessions: end-state design

Decision doc. Compares the session layer as shipped against the proposed
"from scratch" ordering, one decision at a time, with the code that would
change. Each decision ends with a recommendation. Companion user-facing page:
[Agent sessions](./pages/agent-sessions.md).

## Context

What exists today (shipped, tested, live-verified):

- `ai.run.AgentSession<T>` — task↔conversation pairing with
  `of / send / resume / answer / ask / after / fork / save / restore`.
- Public `ai.require_done` + lossless `ai.IncompleteRun`;
  `ai.internal._require_done` deleted.
- Sessions are additive: `Agent` still has its `conversation` field, and
  session methods inject the continuation by copying the runner with a spread.

The proposal under evaluation:

1. Runner = pure policy; `conversation` never appears as a runner parameter.
2. Session as the primary continuation object; a total internal `AgentStart`
   union (`Fresh | Resume | Next | Answer`) as the wire between session
   methods and one run loop; outcomes stamped with their successor session.
3. The two poles — exact sessions versus portable histories — explicit in
   types, with movement between them always a visible import.

The skeleton is not in question. `Task<T>` as the sole typed contract,
providers as `begin/step/submit` capabilities with zero loop ownership, the
four-outcome union, tools as plain functions, and opaque conversation values
all stay exactly as they are.

---

## D1 — Remove `conversation` from the runner

The `conversation: ai.Conversation?` field on `Agent<T>` is the original
ambiguity: it makes one object carry both policy (budget, tool limits,
observers) and state (which continuation to run), and it is why "resume"
versus "new turn" used to be an unstated property of what the caller mutated
beforehand.

**Today** — `Agent.run` starts by disambiguating its own field, and sessions
inject state by cloning the runner:

```baml
// stdlib, Agent.run:
let selected: ai.Provider = match (self.conversation) {
    let existing: ai.Conversation => existing.provider(),
    null => task.provider,
};
// ...
let conversation = match (self.conversation) {
    let existing: ai.Conversation => existing,
    null => provider.begin<T>(task),
};

// stdlib, AgentSession.send — the spread hack:
let continued = self.conversation.append_message(message);
self.task.run(runner = Agent<T> { ...runner, conversation: continued })
```

**Proposed** — the runner loses the field; the loop takes an explicit start:

```baml
class Agent<T> {
    budget: ai.Budget?,
    // ...policy only. No conversation field.

    implements ai.Runner<ai.Task<T>> {
        function run(self, task: ai.Task<T>) -> ... {
            self._run(task, _FreshStart {})       // task.run == fresh, always
        }
    }

    function _run(self, task: ai.Task<T>, start: _AgentStart) -> ... {
        // the one loop; see D2
    }
}

// AgentSession.send becomes intent, not surgery:
function send(self, message: ai.Message, runner: Agent<T> = Agent<T>.new()) -> ... {
    runner._run(self.task, _NextStart { conversation: self.conversation, message: message })
}
```

**What breaks.** Seven corpus call sites pass `conversation =` to
`Agent.new` (retry/fallback tests, structured-output continuation tests, the
handoff test, `conversation_append`). Every one becomes a session call and
reads better for it — e.g. the handoff test's
`provider.submit(...)` + `Agent.new(conversation = continued)` collapses to
`session.answer([...])`.

**Recommendation: yes.** This is the load-bearing cleanup. Low risk now that
sessions exist to absorb every displaced call site, and it deletes the spread
hack, which is a latent bug source (a caller-supplied runner that already had
a conversation would silently lose it).

---

## D2 — Internal `AgentStart` union

**Proposed** — private to `ai.run`, never in user code:

```baml
class _FreshStart {}
class _ResumeStart { conversation: ai.Conversation }
class _NextStart   { conversation: ai.Conversation, message: ai.Message }
class _AnswerStart { conversation: ai.Conversation, results: ai.tools.ToolResult[] }
type _AgentStart = _FreshStart | _ResumeStart | _NextStart | _AnswerStart

// Inside Agent._run — the beginning becomes a total match:
let selected: ai.Provider = match (start) {
    let fresh: _FreshStart => task.provider,
    let resume: _ResumeStart => resume.conversation.provider(),
    let next: _NextStart => next.conversation.provider(),
    let answer: _AnswerStart => answer.conversation.provider(),
};
let conversation = match (start) {
    let fresh: _FreshStart => provider.begin<T>(task),
    let resume: _ResumeStart => _checked(resume.conversation),
    let next: _NextStart => _checked(next.conversation).append_message(next.message),
    let answer: _AnswerStart => provider.submit(_checked(answer.conversation), answer.results),
};
```

Two properties fall out. Append and submit move **inside** the execution
boundary, after invariants — no application code ever mutates a conversation
it is about to run. And each variant gets a precise precondition:
`_NextStart` can reject a conversation with pending tool calls (that is an
unanswered handoff, not a place to append user text), which today fails
inconsistently deep inside each provider adapter.

The public surface stays methods. Users see `send/resume/answer/ask`;
the union is the wire, which is why `Fresh` costs nothing to make explicit —
totality is free when nobody has to spell the variants.

**Recommendation: yes, coupled with D1** (they are one refactor). Keep the
union underscore-private.

---

## D3 — Stamp outcomes with their successor session

**Proposed by the excerpt:** every outcome carries `session`, so
"what do I hold to continue?" is answered by the outcome itself.

**The problem is the type parameter.** `Done<T>` is generic; `BudgetReached`,
`Handoff`, and `Interrupted` are not. A useful successor is
`AgentSession<T>` — that is the whole point of the pairing — so stamping
requires one of:

- *Generify the stop outcomes* (`BudgetReached<T>` …). Viral: every match arm
  in every consumer gains a type argument, and `ai.IncompleteRun` — whose
  field is the stop-outcome union — becomes `IncompleteRun<T>`, which then
  infects every catch site. Rejected; see the appendix for why better
  generics inference does not rescue this.
- *Stamp an untyped session core* and re-type on access. Loses exactly the
  compile-time thread the session exists to preserve. Rejected.
- *Have session methods return a wrapper* `SessionTurn<T> { outcome, session }`.
  Type-checks today (the method has `T` in scope), but every consumer pays
  one level of indirection to reach the outcome union it matches on.

**Recommendation: no — and D7 makes the question moot.** With a
self-advancing session there is no successor value to stamp anywhere: the
session you already hold *is* the continuation after every call.

---

## D7 — Self-advancing sessions (accepted)

The successor-threading style (`send` returns an outcome, `after(outcome)`
builds the next session, predecessor goes stale) was chosen on value-
semantics taste. It costs one ceremony line per turn and introduces a
concept — the stale predecessor — that exists only to be warned about.
BAML class fields are mutable (the corpus relies on this: observer traces,
fake conversation state), so the simpler design is implementable with zero
language changes:

**Before (successor threading):**

```baml
let outcome = session.send(ai.ChatMessage.user("tell me more"));
session = session.after(outcome);          // ceremony: rebuild the box
```

**After (self-advancing):**

```baml
let outcome = session.send(ai.ChatMessage.user("tell me more"));
// session already points at the advanced conversation; keep using it
```

`send`, `resume`, and `answer` update `self.conversation` to the outcome's
committed conversation before returning. `after` is deleted. `of` remains
the constructor; `ask`, `fork`, `save`, `restore` are unchanged. One box,
keep talking to it — the SDE1 test.

What is given up, honestly stated:

- *Aliasing.* Two variables holding the same session both observe advances.
  This is ordinary BAML object behavior (same as `Trace`), and branching —
  the only case where independent references matter — already requires
  `fork()` under either style, because conversations mutate in place at the
  provider layer regardless.
- *A visible stale point.* Successor threading made "old state" a distinct
  variable. In practice every consumer immediately overwrote it; the memory
  agent never used a predecessor after continuing. The warning label was
  protecting a pattern nobody wrote.

Interaction with the other decisions: unchanged. D1/D2 (runner purity,
internal `_AgentStart`) compose identically — the session mutates itself
around the same internal run entry. D4's `fork()` is unaffected and remains
the branching story.

**Recommendation: yes — implemented.** This supersedes `after()` and closes
D3 permanently.

---

## D4 — Fork by value reuse versus explicit `fork()`

The excerpt wants sessions "forkable by value reuse." Empirically,
`Conversation.append_messages` **mutates in place on every provider, real and
fake** — the shipped fork test proved that reuse-after-send corrupts the
sibling branch. Value-reuse forking therefore requires copy-on-send:

```baml
// COW variant of send — predecessor untouched, reuse becomes a safe fork:
function send(self, message: ai.Message, runner: Agent<T> = Agent<T>.new()) -> ... {
    let own = baml.deep_copy(self.conversation);
    runner._run(self.task, _NextStart { conversation: own, message: message })
}
```

Cost: one deep copy of a growing history per turn — O(n²) over a session —
plus copies of sealed provider state. The shipped alternative is linear
sessions with explicit `fork()` (a deep copy you ask for), which has zero
per-turn cost and makes branching greppable intent.

**Recommendation: keep linear + `fork()`.** The COW change is deliberately
localized (one line inside `send`) so this decision is reversible with usage
data. If reuse-after-send footguns show up in user code, flip it then. What
is *not* acceptable is the excerpt's wording as stated — value-reuse forking
without COW is exactly the corruption the test caught.

---

## D5 — The two poles, explicit in types

Exact continuation (provider-pinned, signature-preserving) and portable
history (provider-crossing, lossy-marked) both exist, but movement between
them is scattered: `conversation.messages()` here, `import_messages` there.

**Proposed** — name the poles on the session, movement always visible:

```baml
/// The portable projection of this session's conversation. Crossing this
/// boundary is visible and lossy-marked: exact provider state (thinking
/// signatures, server-side response state) does not survive export.
function export(self) -> ai.Messages

/// Seeds a session for `task` from a portable history, via the provider's
/// ConversationImportProvider capability. The dual of `export`; the pair is
/// the only way to move a conversation between providers.
function import(task: ai.Task<T>, messages: ai.Messages) -> AgentSession<T>
```

```baml
// Cross-provider move — two visible lines, no silence:
let portable = openai_session.export();
let on_claude = ai.run.AgentSession<Resolution>.import(claude_task, portable);
```

`ConversationFidelity` already exists to mark what survived; `import` should
surface it rather than invent anything.

**Recommendation: yes.** Small, additive, and it completes the story the
fork docs already tell ("both poles are legitimate — choose by which
property you need").

---

## D6 — Strict correlation in `answer()`

The one thing worth copying from the AI SDK's tool-approval hardening,
translated to language level: results must bind to the exact pending calls.
Today each provider adapter validates correlation with its own wording at
submit time. Hoist one uniform check into `answer` before submit:

```baml
function answer(self, results: ai.tools.ToolResult[], runner: ...) -> ... {
    //# Every pending call answered exactly once, no extras, before submit
    let pending = self.conversation.pending_calls();   // new Conversation accessor
    let _ = ai.run._require_exact_correlation(pending, results);   // typed failure
    // ...submit + continue as today
}
```

**Recommendation: yes**, contingent on adding a `pending_calls()` accessor to
the conversation protocol (today only individual adapters know their pending
state). If that accessor is deferred, keep relying on adapter-level checks —
do not duplicate a weaker check.

---

## D8 — Continuation surface: three verbs, no `ask` (accepted)

User review of the shipped surface produced the same confusion twice in a
row: `answer` read as "the model answers" (proposed renames: `add_context`,
which misdescribes a mandatory correlated submission as optional enrichment),
and `ask` read as the request half of an ask/answer pair it was never part of
(proposed rename: `complete_task`, which names an object that never
completes — the task is a reusable recipe; turns complete, and the method
throws precisely when one doesn't). Repeated confusion about the same
cluster is evidence against the cluster, not the labels.

The rule that resolves it: **every session method must add a capability, not
a shortcut.**

| Method | Capability |
| --- | --- |
| `send(message)` | continue with a new turn |
| `resume()` | continue without appending |
| `submit_tool_results(results)` | correlated submit + continue |
| `ask(message)` | **none** — exactly `require_done(send(message)).value` |

So:

- **`ask` is removed.** The value-only case was first respecified as the
  visible idiom `ai.require_done(session.send(msg)).value`; further review
  rejected that too (three chained concepts for one value) and D10 replaced
  it with the `complete` verb.
- **`answer` is renamed `submit_tool_results`.** It joins existing
  vocabulary (`provider.submit`, realtime `submit_tool_results`) and is
  fully self-documenting at the call site. The brevity budget is spent
  correctly: every-turn verbs stay short; the rare, careful handoff path
  gets the name you read twice.

Final surface: `send` / `resume` / `submit_tool_results` for continuation —
all returning the same four-outcome union — plus `fork` / `save` / `restore`
for state. Three continuation verbs, one per thing the model might be
waiting for.

---

## D9 — State-guarded continuation methods (accepted, gated on D6)

Motivating idea from review: what if each outcome returned an object with
only its legal continuations — `Done → {send}`,
`Handoff → {submit_tool_results}` — so the wrong call is unrepresentable?

Compile-time typestate fails twice:

1. *The generics wall again.* `Done<T>` could carry a typed
   `ReadySession<T>`, but `BudgetReached`/`Handoff`/`Interrupted` are
   non-generic — per-state session types on outcomes is D3 in costume, and
   fails the same way.
2. *States are subsets, not singletons.* The paused states legitimately
   allow two moves: `resume` (keep going) and `send` (steer — "stop doing
   that, try X"). The memory-agent REPL depends on steering: after an ESC
   interruption, the user's next message is `send` into the interrupted
   conversation. A one-button API forbids a load-bearing pattern.

The version that keeps the value is **runtime typestate**: one session,
three methods, each validating conversation state with a typed error whose
message teaches the state machine:

- `send` on a conversation with pending tool calls →
  "this is an unanswered handoff; call `submit_tool_results`"
- `submit_tool_results` with nothing pending →
  "no handoff to answer; did you mean `send`?"
- `resume` on a completed turn → "nothing to resume; the last turn finished"

This is D6's `pending_calls()` accessor consumed by all three methods, not
just `submit_tool_results` — the two decisions land together.

Related, from the same review: asynchronous control (interrupting a running
loop, injecting or blocking tool calls mid-loop, queueing messages during a
turn) stays **outside** the session by design — cancel tokens and tool
callbacks are runner policy; a message queue is application state. The
session is the between-runs object; nothing may touch a run in flight,
because that boundary is what makes outcomes committed and resumable. One
small addition worth taking: `send` accepting `Message[]` so a drained queue
can become one model turn instead of several.

---

## D10 — `complete`: the run-to-value verb (accepted)

The value path went through three names before landing:

1. `ask(message)` — rejected (D8): read as the request half of an
   ask/answer pair it was never part of, and duplicated `send`'s action
   with only the return contract changed.
2. `ai.require_done(session.send(msg)).value` — rejected: three chained
   concepts to get one value; stdlib-flavored naming in the hottest path.
3. `complete_task` (user proposal) — refined rather than rejected. The
   initial objections mostly failed scrutiny: *"it throws when it can't
   complete"* is the normal contract for verbs (`connect`, `parse`);
   *"the task never completes"* is pedantry — completion is a verb of
   process, not consumption. The one surviving objection — `task` as a
   noun inside a method name collides with the `Task<T>` object — points
   at the fix: make `complete` the verb and the task/session the receiver.

`execute` was also considered and rejected: it is a synonym of the existing
`run` (recreating the indistinguishable-verb-pair problem), carries a
fire-and-forget/void connotation from `ExecutorService.execute` and needed
disambiguating suffixes even in JDBC, and severs the naming thread —
`complete()` throwing `IncompleteRun` is the verb and its negation;
`execute()` throwing `IncompleteRun` is a non-sequitur.

**The shape:**

```baml
// One-shot: direct-call semantics WITH configuration — the gap that made
// require_done necessary in the first place.
let plan: Itinerary = PlanTrip@task(request).with_provider(claude).complete();

// On a session: one more turn, demand the finished value.
let revised: Itinerary = session.complete(ai.ChatMessage.user("add a rest day"));
```

Public `ai.require_done` is removed; `ai.IncompleteRun` stays as the
lossless failure both `complete`s throw (catch it, `AgentSession.of(task,
incomplete.outcome)`, continue — a demanded completion never destroys the
partial run, it only routes it to the error channel).

**The mental model this fixes in place:** tasks are stateless recipes and
can be completed many times (independent runs via `task.complete`,
successive turns via `session.complete`); *runs* have partial-completion
states, and those are exactly the three non-Done outcomes, held by
sessions, each with its continuation verb. Nothing named "session" or
"task" ever completes — turns and runs do.

**Prior-art check** (why this shape, briefly): partial-result
representation by union of typed states is the best-precedented design
(Rust `Poll::Ready|Pending`, `GeneratorState`, the JS iterator protocol,
Pydantic AI's deferred-tools output union) versus discriminant-field bags
(AI SDK `finishReason` — the flag style `ToolResult` already migrated away
from) or partial-as-exception-only (OpenAI Agents SDK `MaxTurnsExceeded`,
which loses the run). Unwrap verbs elsewhere are mostly lossy
(`unwrap`, `getOrThrow`, `Future.get`); carrying the full resumable
outcome inside the thrown failure is the uncommon property worth keeping.
One recorded caveat: Java's `CompletableFuture.complete(value)` is the
*producer* operation, so Java readers may blink once at `task.complete()`;
the receiver and `-> T` signature disambiguate.

---

## D12 — Errors report facts; callers make judgments (accepted, implemented)

`ai.Failure` had two methods: `effects()` (a fact — what this attempt may
have committed, which only the failing layer can know) and `is_transient()`
(a prediction — "could an identical attempt succeed?", which is retry
*policy*). Review verdict: the prediction does not belong on the error. An
error's self-report of transience is unreliable in both directions — a
rate limit is "transient" but terminal once your budget is gone; an invalid
request is "terminal" but retryable after the app fixes it — and every
mature retry library (tenacity's `retry_if`, Polly's `Handle<T>`,
resilience4j predicates) puts that decision in the caller's hands.

Changes:

- `is_transient` removed from `interface Failure` and all ~14
  implementations. The interface is one method: `effects()`.
- `ai.retry(provider, attempts, retry_if = null)` — the caller's judgment
  as an optional predicate `(ai.Failure) -> bool`. Null replays every
  effect-safe failure up to the cap.
- Effect safety remains enforced unconditionally: `Committed`/`Unknown`
  effects are never blind-replayed regardless of the predicate. Facts gate
  safety; judgment gates worth.
- Custom errors keep their own data (`VendorQuotaExceeded.transient` stays
  as a *field*) and the application's predicate reads it — the error
  carries data, the caller decides.

The doctrine line now in the failure protocol docstring: *errors carry
FACTS; callers make judgments.*

---

## Migration summary

| Decision | Verdict | Touches | Risk |
| --- | --- | --- | --- |
| D1 runner purity | **Yes — implemented** | `Agent` class + 7 corpus sites + session methods | Landed with D2/D11 |
| D2 internal `_AgentStart` | **Yes — implemented** | `Agent.run` internals only | Landed; `_FreshStart/_ResumeStart/_NextStart/_AnswerStart` |
| D3 outcome stamping | **No** — mooted by D7 | — | Avoided generics cascade |
| D4 COW forking | **No** (keep `fork()`) | — | Reversible later, one localized line |
| D5 export/import | **Yes — implemented** | `session.export()` / `AgentSession.import` | Landed |
| D6 strict correlation | **Yes — implemented** | `pending_calls()` (nullable; fakes + client report, real adapters pending) + exact-correlation gate | Landed |
| D7 self-advancing session | **Yes — implemented** | `agent_session.baml`, `after()` call sites, docs | Low — full suite covers |
| D8 three-verb surface | **Yes — implemented** | drop `ask`, rename `answer` → `submit_tool_results` | Low — few call sites |
| D9 runtime state guards | **Yes — implemented** | send/submit guards + derived `session.phase()` | Landed; guards skip when the provider reports null |
| D10 `complete` verb | **Yes — implemented** | `Task.complete`, `AgentSession.complete`, remove public `require_done` | Low — mechanical sweep |
| D12 facts vs judgments | **Yes — implemented** | remove `is_transient`; `ai.retry` gains `retry_if` | Low — one gate, ~14 impls |

Status: **every accepted decision is implemented** — D7/D8/D10 first, then
D1+D2+D11 as the loop refactor (with the `Failed` fifth outcome and the
append-is-first-commit rule), then D5+D6+D9, plus the structural contract
fingerprint from the review. D3 and D4 remain closed with recorded
rationale. Follow-ups tracked in internal/alignment-plan.md: real-adapter
`pending_calls()` overrides and wiremock-level atomicity tests. The session surface is
`send / complete / resume / submit_tool_results / fork / save / restore`:
one box you keep talking to; `send` tells you what happened, `complete`
demands the finished value, and the other verbs each answer one thing the
model might be waiting for.

---

## Appendix: would better generics change these answers?

Asked during review: with full generic inference, does D3 become the simpler
design? Split the answer by feature:

**Inference alone: big surface win, no design change.** Constructor type
arguments are most of today's visible ceremony (`ai.run.Agent<Resolution>
.new(...)`, `AgentSession<string>.of(...)`, `require_done<T>(...)`).
Inference erases them at every call site. Worth wanting regardless of any
session decision.

**Inference does not rescue stamped-generic outcomes.** Two walls are
structural, not notational:

1. *Generic error types need existentials at the catch site.* An
   `IncompleteRun<T>` can only be caught where `T` is statically known.
   Middleware that catches failures from several agents — retry wrappers,
   loggers, a REPL's catch-all — needs `IncompleteRun<?>` (a wildcard:
   "of some T"), which is a type-system feature, not inference. And where
   `T` *is* statically known, the caller already holds the typed session,
   so the stamp adds nothing.
2. *Shared plumbing loses its common type.* One non-generic `BudgetReached`
   serves every agent today. Generified, `BudgetReached<string>` and
   `BudgetReached<Itinerary>` are unrelated; collecting them again needs
   wildcards plus variance rules — and `AgentSession<T>` is invariant
   (`T` appears in both argument and return positions), so no sound common
   supertype exists to fall back on.

**The features that actually simplify the surface are cheaper than
generics.** Destructuring would make a `SessionTurn<T>` wrapper one clean
line; a self-advancing session needs no language change at all. D7 takes the
zero-cost option. Ranked wish list for the language: constructor type-arg
inference, then destructuring, then existentials — and only the last would
reopen D3, at which point D7 will have already removed the need.
