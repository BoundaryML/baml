# Reference implementation notes

The working reference lives at `_plan/ai_agents/` — see its `readme.md`
for the layout (library in `ns_ai/`, one scenario file per doc page).
Status: `baml check` clean, 30 offline tests pass, live OpenAI loop
verified end to end (`infisical run --env=test -- baml run -e 'demo()'`:
the model calls both tools through `reflect.call_any` and returns a
typed `Itinerary`).

Notably working **today**, verified by the suite:

- **Typed sessions**: `Session<T, X>`, `Done<T> { result: T }`, and union
  goals — `Session<Itinerary | CannotPlan, never>` deserializes whichever
  variant the model produced (s09).
- **Typed custom event unions, no escape hatch**: `X` is the custom-event
  extension; the machinery runs on `Event | X`, so built-ins are members
  by construction and no lower-bounded generics are needed. Policies bind
  `type Ev = root.ai.Event | ApprovalExt`; `send_event(PermissionGranted
  { ... })` is compile-checked; custom events round-trip through
  snapshots typed (s06, s07). A `Custom { kind, data_json }` fallback is
  not needed and was removed.
- **User-defined error classes**: `StepBudgetExceeded` /
  `CostBudgetExceeded` / `SessionInterrupted` are plain classes — thrown
  from generic policy/session frames, caught with typed arms, fields
  intact (s03, s06, s09). No marker interface or naming convention.

## Bugs hit (toolchain 0.15.1-nightly.20260727)

1. **`baml fmt` crashes on `client` as a field/parameter name.**
   `Expected token/node of kind WORD, but found KW_CLIENT`
   (`ns_ai/session.baml` field `client: Client`, `shared/plan_trip.baml`
   param). Checker and runtime accept it; only the formatter dies.
2. **Comment placement is inconsistent.** Comments inside `interface`
   bodies are parse errors. `//#` is rejected in class bodies, match-arm
   position, and inside array literals, while plain `//` is fine in class
   bodies and statement positions. Comments between array elements are
   rejected entirely.
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
   remaining generics gap is type aliases (bug 3).

## Spec issues the implementation surfaced (feed back into the BEP)

1. **Policy appends vs. resume-by-refold.** `WithSteering` appends
   flushed messages inside `update`; resume rebuilds state by re-folding
   the journal through `update`. Refolding an appending policy would
   mutate history. Resolution needed: appends become a command
   (runner-mediated), or refold runs append-suppressed.
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
5. **Task mode's step budget must count reply-only turns**, or a chatty
   model loops forever under the tool-step budget (`ToolLoop.task_mode`).
6. **The event type parameter should be the EXTENSION, not the full
   union.** `Session<T, E>` with builtin events upcast into bare `E` is
   unsound (rigid type variables have no lower bound), and the checker
   correctly rejects it. Parameterizing by the extension `X` and running
   the machinery on `Event | X` gives the same expressiveness with no
   new language features — `never` is the empty extension, and
   `Event | never` absorbs so plain clients still satisfy default
   bindings. The BEP's `@session` desugaring should adopt this shape.
7. **Journal folds are order-sensitive.** The docs' `pending_permission`
   example had a single-pass fold that missed answers arriving after
   requests — caught by scenario s07's test, fixed in the example
   (two-pass). Doc examples backed by running tests earn their keep.
