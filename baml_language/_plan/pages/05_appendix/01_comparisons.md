# Comparisons

How the concepts in this BEP map onto other systems. The goal is
orientation, not scorekeeping.

## Pydantic AI

| Pydantic AI | BAML |
|---|---|
| `Agent('openai:gpt-5.2', output_type=Itinerary)` | `function PlanTrip(...) -> Itinerary { client: ... }` |
| `@agent.tool` decorated function | any function in `tools: [...]` |
| `system_prompt=` string in Python | `prompt:` block, compile-checked |
| `agent.run_sync(...)` → `result.output` | `PlanTrip(...)` → the value |
| `message_history=result.new_messages()` | session-managed; `${ctx.transcript}` |
| `ModelMessagesTypeAdapter` persistence | `s.snapshot()` → one string |
| `RunContext` / `deps` | closures capture what they need |

Differences that matter: BAML persists typed events (the journal) rather
than provider messages, which is what enables cross-provider resume and
built-in tracing; `Done | Replied` distinguishes goal-reached from
conversation-continuing, which `run()`-always-has-output does not; prompts
and schemas are checked at compile time.

## OpenAI Agents SDK

| OpenAI Agents SDK | BAML |
|---|---|
| `Agent` + instructions | LLM function |
| `Runner.run(agent, input)` | `run()` on a session (the runner is internal) |
| `final_output` | `Done<T>.result` |
| `Session` | session with a journal |
| handoffs | subagent calls |
| guardrails | policies / middleware |

The SDK ties session state to its own storage and provider; BAML sessions
serialize to a string and re-render per provider.

## Flue

Flue is the closest system in operational rigor. Shared conclusions,
reached independently: a durable canonical stream per conversation with a
separate ephemeral observability stream; subagents as isolated child
sessions with their own durable records; tool argument validation before
execution; tool errors returned to the model; async submission with
receipts and exactly-one-settlement.

Where the designs diverge:

- **Typed goals.** A Flue agent function returns its system prompt; the
  conversation has no typed outcome. A BAML agent returns `T`, and the
  loop's termination condition is producing it.
- **Re-render vs. policy.** Flue re-runs the agent function before every
  model call; capabilities are mounted conditionally during render
  (`if (approved) useTool(publish)`). BAML keeps the function static and
  makes capability changes policy commands recorded in the journal
  (`MountTools`), so the cause of every change is in the history. See
  `02_design_principles.md`.
- **Extension seam.** Flue's harness loop is fixed; configuration happens
  through hooks. BAML exposes the policy layer, so injection timing,
  approvals, and budgets are user-definable and testable.
- **State location.** Flue conversations live in a database adapter; BAML
  journals are values that serialize to a string, with stores as an
  option for named instances.

Adopted from Flue's design, with credit: atomic commit seams for state,
receipts and settlement outcomes, named conversation instances with
create-only semantics, and step checkpoints inside durable tools.

## LangGraph

LangGraph models an agent as a state graph with a checkpointer. BAML has
no graph DSL: control flow is code, and the journal — the analogue of the
checkpointer — is the primary object rather than a plugin. A graph can be
built on a log; a log cannot be recovered from a graph.
