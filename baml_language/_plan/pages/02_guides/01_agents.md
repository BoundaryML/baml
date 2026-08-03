# Agents

## An LLM function is a typed function

An LLM function has a prompt for a body and a return type that the model
must produce:

```baml
function ExtractRecipe(text: string) -> Recipe {
    client: "openai/gpt-5.2"
    prompt: `
        Extract the recipe from this text.
        ${text}
        ${ctx.output_format}
    `
}
```

The return type is the schema. `${ctx.output_format}` renders it into the
prompt; the parser validates and repairs the model's output before your
code sees it.

Two placeholders matter for agents:

- `${ctx.output_format}` — the return type's schema.
- `${ctx.transcript}` — the conversation so far, rendered natively for
  the configured provider. Include it in any function used with tools or
  sessions. In a fresh one-shot call it renders as empty.

## An agent is a function with tools

```baml
function PlanTrip(trip_request: string) -> Itinerary {
    client: "openai/gpt-5.2"
    tools: [search_flights, search_hotels]
    prompt: `
        You are a travel agent. The brief: ${trip_request}
        ${ctx.transcript}
        ${ctx.output_format}
    `
}
```

There is no separate agent declaration. The `tools:` list turns the
function into an agent; the return type is the agent's goal.

## Task mode

Calling an agent function runs it in task mode: the model works in a loop
until it produces the return type.

```baml
let trip = PlanTrip("2 weeks in Japan, mid-range budget");   // -> Itinerary
```

Each iteration is one model turn:

1. The client renders the journal and calls the model.
2. The model produces the return type, calls tools, or writes an
   intermediate note.
3. Tool calls run — concurrently within a turn — and their results are
   appended to the journal.
4. Repeat.

In task mode the model cannot stop and wait for input. A conversational
reply is recorded and the loop continues. When the agent needs to talk to
a user, use a session (`02_sessions.md`). When it should run detached,
use a job (`../03_examples/02_background_jobs.md`).

## Configuring a run

Configuration parameters share the call parentheses with the function's
arguments, distinguished by a `$` prefix. Bare names go to the function;
`$` names go to the runtime:

```baml
let trip = PlanTrip("2 weeks in Japan", $max_steps = 20);
```

Function parameters cannot start with `$`, so a function with its own
`max_steps` parameter works unchanged. `03_configuration.md` covers the
full set of `$` parameters, mid-run setters, and precedence.

## Step budgets

Every task has a step budget; the default is 12 model turns. Exhausting
it throws `baml.session.StepBudgetExceeded`.

## Errors

- A tool error becomes a tool result. The model sees it and adapts.
- Provider failures throw `baml.errors.*` after the client's retry policy
  is exhausted.
- Handle failure at the call site:

```baml
let trip = PlanTrip(trip_request) catch_all (e) {
    let t: baml.session.StepBudgetExceeded => fallback_itinerary(trip_request),
    _ => throw e,
};
```

## Every call is recorded

A task records a journal even when you never see it as a session — every
model turn, tool call, and token count, in order. The journal is the
trace. See `11_journal.md`.

An LLM function is the only place BAML talks to a model. Code that
reaches a provider through raw HTTP gets none of this — no journal, no
snapshot, no replay.
