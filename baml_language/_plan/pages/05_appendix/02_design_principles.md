# Design principles

The rules the rest of this BEP follows, and the alternatives that were
considered and rejected. Cite these in review by name.

## The function defines the turn; the policy defines the session

An LLM function is a static template: prompt shape, return type, initial
tools. Everything that changes mid-session — mounted tools, injected
messages, budgets, modes — changes imperatively in the policy, through
commands, and is journaled.

Rejected alternative: re-rendering the function per turn against session
state (Flue's model), e.g. `tools: if (state.approved) { [...] } else { [...] }`.
Rejected because it creates two sources of truth for capabilities (the
expression and the policy), hides the cause of changes from the journal
(the effect is visible, the reason is not), adds a determinism obligation
on arbitrary expressions for replay to work, and entangles the function
with session state so it no longer works as a plain one-shot call.

## The journal owns the data

Clients are stateless codecs. Policy state is a derivable cache. The
runner holds nothing. One consequence checked in review: any proposed
feature that stores conversation state outside the journal must instead
record events and derive.

Rejected alternative: provider-message arrays as the persisted state
(Pydantic AI, OpenAI SDK). Loses tool/usage/child structure, ties state to
one provider's format, and makes cross-provider resume impossible without
lossy conversion.

## Two lanes

Data (`send`) queues and the policy chooses injection timing. Control
(`interrupt`) preempts through cancel tokens and is recorded after taking
effect. Rejected alternative: interrupts as ordinary queued events —
a cancel behind ten queued messages is not a cancel.

## Static templates, imperative changes, recorded causes

Mid-session variation goes through one door: events and commands in the
journal. Prompt-visible mode changes are `Promptable` events, not template
edits. Capability changes are `MountTools` / `UnmountTools` commands, not
conditionals in the function.

## Streaming is not history

Token deltas travel on an ephemeral channel and are never journaled. The
journal records final messages. A UI built only on the journal tail is
correct; the stream is cosmetic.

## Configuration is not an argument

Call parentheses hold exactly the function's declared parameters,
whether calling plainly, `@session`, or `@job`. Run configuration —
`max_steps`, `policy`, `client`, `tools`, `id`, `new`, `resume` — lives
in a `with baml.session.options(...)` clause, mirroring
`spawn with baml.spawn.options(...)`.

Rejected alternative: configuration as reserved keyword arguments on the
call (`PlanTrip(request, max_steps = 20)`). It collides with the
function's own parameter namespace — any function with a `max_steps` or
`id` parameter becomes uncallable with configuration — and the collision
set would grow with every future option.

## Tools are plain functions

No decorators, no schema files, no context parameters. Schemas come from
signatures and docstrings via reflection (BEP-062); validation is
`reflect.call_any`. Session interaction goes through ambient functions
(`baml.session.emit`, `baml.session.step`) that no-op or degrade
gracefully outside a session, so the same function is a tool, a workflow
step, and a unit under test.

Rejected alternative: an injected context parameter on tools
(`fn tool(ctx: Ctx, ...)`). It leaks the session into every signature,
makes tools non-reusable outside sessions, and forces reflection to
special-case the parameter when presenting schemas to the model.

## One journal per session; sessions form a tree

Child sessions have their own journals, linked by `child_id`. Rejected
alternative: one flat log with correlation IDs. Global ordering is
occasionally convenient, but per-session replay and compaction become
unbounded, and exporting a single delegation's transcript requires
filtering the world. Stores may still physically co-locate journals;
the tree is the semantic model, not a storage mandate.

## Determinism where it pays, honesty where it does not

Committed work never re-runs; uncommitted work re-runs at-least-once. The
journal records that effects ran, never the effects themselves — external
side effects must be idempotent, keyed on stable IDs. This BEP does not
require full deterministic replay of arbitrary agent code; the journal
format is designed so a stricter tier can be added without breaking
existing journals.
