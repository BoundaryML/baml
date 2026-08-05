# Reference implementation notes

The working reference lives at `_plan/ai_agents/` — see its `readme.md`
for the layout (library in `ns_ai/`, one scenario file per doc page).
Status: `baml check` clean, 37 offline tests pass, live OpenAI loop
verified end to end (`infisical run --env=test -- baml run -e 'demo()'`:
the model calls both tools through `reflect.call_any` and returns a
typed `Itinerary`).

2026-08-03: alignment pass between the docs and the reference. The
reference now implements the doc-specified behavior it previously
diverged from: setters journal `ClientChanged`/`PolicyChanged` and
`set_policy` refolds the journal; tool errors and argument-validation
failures become `ToolFailed` events (they were error-string
`ToolCompleted`s); `CancelAll` fires cancel tokens and is not a
turn-ender (10_policies runner spec) — the default policy answers an
interrupt with `[CancelAll, CallModel]` so the model reacts
(04_steering); the step budget counts model calls, not tool
completions; `send` is one verb taking `string | X`; `with_retry`
middleware exists; `Promptable` opt-in transcript rendering works via
interface match. The docs were updated where the reference was right:
the session type parameter is the extension (`Session<T, X>`, appendix
§10), `with_budget` throws a typed `CostBudgetExceeded`, and setter
events are audit history, not stored values (03_configuration).

Same day, second pass: the refold contract (spec issues 1 and 8) was
resolved, implemented, and written into 10_policies ("The refold"):
`update` never writes the journal (`j.record` for journal-only decision
records, `FailTool` for the denial case), the refold skips recorded
entries and swallows throws, and `WithSteering`'s flush became a pure
state transition. New tests: kill-the-process mid-approval then resume
(s06), and reviving an exhausted session (s09).

Notably working **today**, verified by the suite:

- **Typed sessions**: `Session<T, X>`, `Done<T> { result: T }`, and union
  goals — `Session<Itinerary | CannotPlan, never>` deserializes whichever
  variant the model produced (s09).
- **Typed custom event unions, no escape hatch**: `X` is the custom-event
  extension; the machinery runs on `Event | X`, so built-ins are members
  by construction and no lower-bounded generics are needed. Policies bind
  `type Ev = root.ai.Event | ApprovalExt`; `send(PermissionGranted
  { ... })` is compile-checked; custom events round-trip through
  snapshots typed (s06, s07). A `Custom { kind, data_json }` fallback is
  not needed and was removed.
- **User-defined error classes**: `StepBudgetExceeded` /
  `CostBudgetExceeded` are plain classes — thrown from generic
  policy/session frames, caught with typed arms, fields intact (s06,
  s09). No marker interface or naming convention.
- **The appendix §1 `$runner` typing rule**: a generic interface with an
  associated type (`interface SessionRunner<Out> { type Handle }`) and a
  bounded generic open site
  (`open_with<Out, R extends SessionRunner<Out>>(...) -> R.Handle`)
  typecheck and dispatch today — the same expression yields
  `Session<T, never>` under `BlockingRunner` and `Job<T>` under a job
  runner (`ns_ai/runner.baml`, s10). The projection the appendix calls
  "the first thing the reference implementation must validate" works.
- **`Promptable` via interface match**: `match (e) { let p: Promptable
  => p.to_prompt() ... }` works, including when the scrutinee's type is
  a rigid generic (`render_transcript<E>`), so opt-in custom-event
  rendering — and with it the skills recipe — needs no new language
  features (s07).
- **One `send` verb**: a parameter typed `string | X` (X the rigid
  custom-event extension) matches and narrows correctly, so
  `s.send("...")` and `s.send(PermissionGranted { ... })` are one method
  (s03, s06, s07).

## Bugs hit (toolchain 0.15.1-nightly.20260727)

1. **`baml fmt` crashes on `client` as a field/parameter name.**
   `Expected token/node of kind WORD, but found KW_CLIENT`
   (`ns_ai/session.baml` field `client: Client`, `shared/plan_trip.baml`
   param). Checker and runtime accept it; only the formatter dies.
2. **Comment placement is inconsistent.** Comments inside `interface`
   bodies are parse errors. `//#` is rejected in class bodies, match-arm
   position, and inside array literals, while plain `//` is fine in class
   bodies, statement positions, and match-arm position. Comments between
   array elements are rejected entirely.
3. **Generic type aliases do not parse.** `type Turn<T> = Done<T> | Replied`
   → `a function type cannot declare generic parameters`. Generic classes
   and generic functions work; aliases do not. Workaround: inline the
   union in signatures.
4. **Qualified struct literals misparse inside array literals.**
   `[root.ai.ToolRequested { ... }]` → `expected expression, found ']'`.
   Fine at let/argument positions. Workaround: factory functions
   (`root.ai.tool_requested(...)`) — wanted anyway as the
   `baml.session.user(...)` idiom, but the parse gap is real.
5. **Expression statement starting with `[` after a closing `}` parses as
   a subscript** (JS-ASI-style). Bare `[]` as a tail expression after an
   `if` block fails (`cannot index into type void`); `return [];` works.
6. **No divergence analysis for `while (true)`.** A loop whose every exit
   is `return` still triggers `E0029 missing return expression`; needs an
   unreachable tail value.
7. **`baml.json.to_string<T>` fails at runtime when `T = unknown`**
   (`cannot serialize unknown type`) even when the dynamic value is
   serializable — while `baml.json.to_json` walks it fine. Bites
   immediately when combining `reflect.call_any` (returns `unknown`) with
   JSON tool results. Workaround: `stringify(to_json(v))`. Should either
   use the dynamic type or fail at compile time.
8. **Diagnostic nit:** `testset foo {` (bare identifier) reports
   `E0003 unresolved name: foo`; the actual rule is that testset names
   are quoted strings.
9. **`baml run -e` VM error on generic frames.** The exact code that
   passes in a file function fails in eval:
   `VM internal error: could not realize type template: template
   references frame type-arg slot 0 but the frame has 0 type args`
   (repro: construct `Session<T, X>`, interrupt, `run() catch_all` — as
   an eval expression only).
10. **`.filter()` on a generic receiver infers `Entry<never>[]`** where
   `Entry<E>[]` is expected (`Journal<E>.read_from`); a manual loop works.
11. **Assoc-type unification through a narrowed match arm fails.** Inside
   `match (e)`'s `let a: AssistantMessage =>` arm, calling
   `self.inner.update(st, j, e)` (inner: `Policy<Ev = Event | X>`) infers
   `Ev = Event | AssistantMessage` from the narrowed argument and rejects
   the journal. Widening explicitly (`let ev: Event | X = a`) fixes it.
12. **Spurious irrefutability warning:** `if let e: Event | X =
   inbox.shift()` (shift returns `(Event | X)?`) warns E0112 irrefutable
   — presumably because rigid `X` could include `null` — but the null
   case is taken at runtime and the loop exits correctly.
13. **An empty class in a union swallows later variants on JSON
   round-trip.** Serialization is untagged; deserialization matches
   structurally, and an empty class matches any object, so every variant
   after it in the union deserializes as the empty class. Bit
   `PolicyChanged {}` in the `Event` union: after `snapshot()` →
   `resume_journal`, all later events (including custom extensions) came
   back as `PolicyChanged`. Workaround: give journaled marker events a
   literal discriminant field (`kind: "policy_changed"`). Empty classes
   are advertised as union variants; either tag the wire format or warn.
14. **No generic parameter defaults.** `class Session<T, X = never>` is
   a parse error, so the docs' single-parameter `Session<Itinerary>`
   cannot exist as a library type; users write `Session<Itinerary,
   never>`. Interface associated types DO take defaults (`type Ev =
   Event`), which makes the omission on classes feel arbitrary.
15. **Literal arguments to bounded-generic calls infer literal types.**
   `open_with(BlockingRunner<int> {}, 42)` fails with `expected
   Runner<42>, found BlockingRunner<int>` — the literal infers as type
   `42` and unification runs against the wrong instantiation. Binding
   the argument to a typed variable first avoids it.

Non-bug: `baml fmt` re-indents backtick string interiors — safe, because
template literals strip common indentation.

## Language features the BEP needs that the library cannot express

1. **Structured output for the loop.** Without `${ctx.output_format}` /
   SAP outside llm functions, the reference speaks a hand-rolled JSON
   protocol and hand-writes the schema string. This showed up as a real
   failure: one live run looped to the step budget because the model
   wrapped its JSON in markdown fences and the naive parse missed it
   (fixed with manual fence-stripping in `shared/parser.baml`). SAP
   absorbs exactly this class of fragility — strongest single argument
   for the language feature.
2. **Ambient session context.** `baml.session.emit` / `step` need dynamic
   scoping the library cannot provide; tools cannot reach their enclosing
   session (s08 fakes it with closures).
3. ~~Custom throwable error types~~ — **work** (see above); nothing
   needed from the language.
4. ~~Class generics / typed event unions~~ — **work** (see above). The
   remaining generics gaps are type aliases (bug 3) and generic
   parameter defaults (bug 14) — the latter is what stands between
   `Session<Itinerary, never>` and the docs' `Session<Itinerary>`.
5. ~~The `$runner` associated-Handle projection~~ — **works** as a
   library (see above, s10); the language sugar still needs the `@`
   desugar to route through it.

## Spec issues the implementation surfaced (feed back into the BEP)

1. **Policy appends and throws vs. resume-by-refold — RESOLVED,
   implemented, and adopted by the docs** (10_policies "The refold").
   `update` no longer writes the journal. The design: (a) journal-only
   custom decision records go through `j.record(e)`, which flags the
   entry and is suppressed while replaying, and the refold skips
   flagged entries — recorded history can never be duplicated or
   double-folded; (b) built-in events stay runner-only — the denial
   case became the `FailTool` command (see issue 8); (c) `WithSteering`
   stopped appending entirely — arrivals are journaled by the runner as
   the admission record, and the flush is a pure state transition that
   recalls the model; (d) policies may still throw to abort a live run,
   and the refold swallows throws per event, so resuming an exhausted
   session works and a fresh message resets the budget. Verified by the
   s06 kill-the-process-mid-approval test and the s09 revival test.
   Remaining caveat for the BEP: with arrival-time journaling, a policy
   that holds messages across several turns needs render-side support
   to keep them out of the transcript until injection; the default
   turn-boundary steering does not hit this.
2. **Ingest ordering contract.** Action events must precede their
   `AssistantMessage` in an ingest batch, or the policy sees the message
   with empty `pending_tools` and ends the turn early. Codified in
   `ns_ai/openai.baml`; the BEP should state it as a client rule.
3. **CallModel coalescing.** Two messages injected at one boundary each
   produce a `CallModel`; the runner must coalesce or the model is called
   twice for one turn. Implemented in `ns_ai/session.baml`; belongs in
   the runner spec.
4. **`call_id` allocation belongs to the runner** — unique per journal,
   stable across retries. The reference mints them in the parser, which
   is only correct for a single client.
5. **The step budget counts model calls.** ~~Task mode's step budget
   must count reply-only turns~~ — resolved by counting the loop's model
   calls (`ToolLoop.next_call`), which covers tool turns and reply-only
   turns uniformly and matches the docs' "counts model turns"
   (01_agents). One edge: the opening call of a `run()` is issued by the
   runner, not the policy, and is uncounted.
6. **The event type parameter is the EXTENSION, not the full union.**
   `Session<T, E>` with builtin events upcast into bare `E` is
   unsound (rigid type variables have no lower bound), and the checker
   correctly rejects it. Parameterizing by the extension `X` and running
   the machinery on `Event | X` gives the same expressiveness with no
   new language features — `never` is the empty extension, and
   `Event | never` absorbs so plain clients still satisfy default
   bindings. Adopted by the docs (11_journal, appendix §10, examples).
7. **Journal folds are order-sensitive.** The docs' `pending_permission`
   example had a single-pass fold that missed answers arriving after
   requests — caught by scenario s07's test, fixed in the example
   (two-pass). Doc examples backed by running tests earn their keep.
8. **Policy-appended built-ins — RESOLVED with the `FailTool` command.**
   The denial case (`WithApproval` telling the model a held call died
   without running it) was the one place a policy appended a built-in
   event. `FailTool { call_id, error }` closes it: the runner appends
   `ToolFailed` and folds it like any tool outcome, so the inner loop
   clears the pending call and recalls the model — identically live and
   on refold, and the gate no longer needs its own `CallModel`. Adopted
   in 10_policies (command list, example) and the reference.
9. **`PolicyChanged` cannot carry the policy.** A policy is a runtime
   value, often closing over state, so the setter event records that a
   change happened, not what it became; resume takes `$policy` from the
   caller. `ClientChanged` differs: clients are declared, so the
   recorded ID is re-resolvable. 03_configuration now states this;
   the BEP should keep the asymmetry explicit.
