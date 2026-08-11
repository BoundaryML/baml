# Concepts

Read this page before the guides. It defines the pieces, the loop that
connects them, and the terms the rest of the BEP uses.

## The pieces

- **LLM function.** A BAML function whose body is a prompt and whose
return type is the schema the model must produce. A `tools:` list
makes it an agent.
- **Spec.** `MyFunc@spec(args)` binds the function's arguments and
returns a `FunctionSpec` value. A spec describes one unit of model
work and performs none of it.
- **Runner.** A value that consumes a spec and drives it to
completion. The built-in `Agent` runner is what a plain call uses.
- **Client.** A value that performs one model turn over one provider
wire API. `"openai/gpt-5.6"` is a client resolved from a string.
- **Journal.** The append-only record of one run. Its events are the
transcript source for the next model turn and the trace of the
finished run.
- **Tool.** A plain BAML function the model may call. Its signature is
its schema.

## The turn loop

```
FunctionSpec ──run──► Agent (runner) ──invoke──► Client ──HTTP──► provider API
                        │                          │
                        │◄──────── ModelTurn ──────┘
                        │   content blocks, stop reason, usage
                        │
                        ├─ commit the turn's events to the journal
                        ├─ execute tool calls, append correlated results
                        └─ repeat until the output parses, then return
                           RunResult { value, journal, usage }
```

One iteration is a model turn. The runner assembles a `ModelTurnInput`
from the spec and the journal and calls `client.invoke`. The client
renders the request in its wire format, makes one HTTP call, and
returns a `ModelTurn` of canonical content blocks. The runner commits
the turn to the journal as one batch, executes any tool calls, appends
their results, and repeats. When a turn's final candidate parses as
the return type, the run ends and the runner returns a `RunResult`.

## Who owns what

The runner owns the loop: tool execution, result correlation, budgets,
journal writes, and the typed parse of the final output. The client
owns one turn: rendering the prompt and transcript into its wire
format, transport, and normalizing the response into content blocks.
The journal owns all state. A client holds no conversation state, so
one client value serves any number of concurrent runs.

The boundary is strict in both directions. A client never writes the
journal, never executes a tool, and never parses the return type. A
runner never sees a wire request or response.

## The two laws

1. **Assistant content is structured blocks.** A model turn is a list
 of `Text`, `Reasoning`, `ToolUse`, and `Media` blocks, never a bare
 string and never a raw HTTP body. Tool results correlate to calls by
 id, and a different client can re-render another client's turns.
2. **The journal alone renders a complete request.** Every model turn
 can be rebuilt from the journal and the spec. Server-held state,
 when a later phase adds it, is an optimization that can always be
 discarded.

Streaming already extends the client boundary without changing either law:
deltas are ephemeral and the completed turn remains canonical. Later phases —
replay fidelity and server-side continuations — extend the BEP the same way
(`../05_appendix/03_future_phases.md`).

## Glossary

- **journal** — the append-only typed record of one run. Not "log" or
"history".
- **transcript** — the journal as rendered into a provider's message
format. A rendering, not a stored object.
- **client** — the value that performs model turns. Not "provider",
except in the phrase "provider wire API" and as a registry prefix.
- **spec** — a `FunctionSpec` value.
- **runner** — a value implementing `Runner` that consumes a spec.
- **run** — one `Runner.run` call, from spec to result.
- **turn** — one `Client.invoke` call within a run.
- **tool** — a function the model may call during a run.
- **content block** — one element of a model turn: `Text`,
`Reasoning`, `ToolUse`, or `Media`.
