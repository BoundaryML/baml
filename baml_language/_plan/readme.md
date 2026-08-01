# BEP: AI Functions and Agents

## Summary

This BEP adds agents and multi-turn sessions to BAML. It builds on the
existing LLM function: a function whose body is a prompt and whose return
type is the schema the model must produce.

The core additions:

- **`tools:`** on an LLM function. The function becomes an agent that
  calls tools in a loop until it produces its return type.
- **Sessions.** `MyFunc@session(...)` creates a long-lived conversation
  from any LLM function. Sessions serialize to a string, resume anywhere,
  and can be addressed by ID.
- **Jobs.** `MyFunc@job(...)` runs a task in the background and returns a
  pollable handle.
- **The journal.** Every run records an append-only log of typed events.
  Snapshots, tracing, replay, evals, and provider portability are all
  derived from it.
- **Policies.** A small, optional API for changing session behavior:
  steering, approvals, budgets, dynamic tool mounting.

Everything is layered. The sugar (`tools:`, `@session`, `@job`) desugars
into a public library (`baml.session.*`). You can drop down to the
library at any point and lose nothing.

## What this BEP adds

| Surface | Kind |
|---|---|
| `tools:` field on LLM functions | language |
| `${ctx.transcript}` in prompts | language |
| `MyFunc@session(...)`, `MyFunc@job(...)` | language |
| `baml.session.*` (Journal, events, Session, Job, Policy, Toolbox) | stdlib |
| Client interface: `render` / `invoke` / `ingest` | stdlib |
| Session and job support in generated SDKs and `baml serve` | tooling |

## What this BEP does not change

- Existing LLM functions work unchanged. A function without `tools:`
  behaves exactly as today.
- `client<llm>` declarations are unchanged. This BEP defines the
  interface they already implement.
- No graph or state-machine DSL. Control flow is ordinary BAML code.

## Reading order

**Introduction** — start here.
`01_getting_started` (zero to agent), `02_why` (motivation),
`03_concepts` (the vocabulary; read before the guides).

**Guides** — one page per concept, in the order you meet them:
`01_agents`, `02_sessions`, `03_steering`, `04_models`, `05_tools`,
`06_mcp`, `07_skills`, `08_subagents`, `09_policies`, `10_journal`,
`11_durability` (design notes), `12_serving`.

**Examples** — the system composed:
`01_claude_code` (steering, permissions, subagents, Esc),
`02_background_jobs` (`@job`, polling, provider background mode).

**Advanced** — `01_errors_and_retries`, `02_evals`, `03_observability`.

**Appendix** — `01_comparisons` (Pydantic AI, OpenAI Agents SDK, Flue,
LangGraph), `02_design_principles` (the laws, and rejected alternatives).

`outline.md` lists every header for reference.
