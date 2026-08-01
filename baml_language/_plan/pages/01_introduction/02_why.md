# Why

## The problem

An agent is a loop around an LLM: call the model, run the tools it asks
for, feed results back, stop when done. Every team writes this loop, and
every team rewrites the same surrounding machinery: conversation storage,
provider message formats, retries, tracing, approvals, cancellation.

That machinery is usually built in an application framework, which means
the language cannot see it. A raw HTTP call to a provider is invisible to
tracing. Conversation state is an array of one provider's message format.
The agent's goal is a comment, not a type.

## The approach

BAML already treats an LLM call as a typed function. This BEP extends the
same idea to agents:

- **The goal is the return type.** An agent is a function that returns
  `T`. The loop terminates by producing it. There is no untyped "agent
  finished" state.
- **Every LLM operation is recorded.** All model and tool calls flow
  through LLM functions, and every call lands in a journal — an
  append-only log of typed events. Tracing is not an integration; it is
  the data model.
- **State is portable.** A conversation serializes to one string, resumes
  on any machine, and re-renders for any provider. The journal stores
  canonical events plus each provider's raw payloads, so switching
  providers mid-conversation is a rendering decision, not a migration.
- **Behavior is separate from prompting.** The function defines the turn:
  prompt, output type, initial tools. A policy defines the session:
  approvals, budgets, steering, capability changes. Policies are pure and
  unit-testable without a model.

## What you do not get

- No graph or state-machine DSL. Control flow is BAML code: loops,
  `spawn`, `match`.
- No hidden re-rendering. The function template is static; everything
  that changes mid-session is a recorded event with a recorded cause.
- No framework lock on the loop. The sugar (`tools:`, `@session`)
  desugars into a public library. You can write the loop by hand and keep
  every guarantee.

## Relation to other systems

The design borrows deliberately: durable logs and settlement from
event-sourcing and durable-execution systems, pure update functions from
Elm-style architectures, isolated child sessions from actor systems.
`../05_appendix/01_comparisons.md` maps the concepts onto Pydantic AI,
the OpenAI Agents SDK, Flue, and LangGraph.
