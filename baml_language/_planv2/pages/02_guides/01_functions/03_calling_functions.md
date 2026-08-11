# Calling functions

This page uses the travel agent:

```baml
function PlanTrip(trip_request: string) -> Itinerary {
    client: "openai/gpt-5.6"
    tools: [search_flights, search_hotels]
    prompt: `
        You are a travel agent. The brief: ${trip_request}
        ${ctx.output_format}
    `
}
```

## Calling runs the default runner

A plain call binds the arguments into a spec, runs it with the
built-in `Agent` runner, and unwraps the value:

```baml
let trip: Itinerary = PlanTrip("2 weeks in Japan");
// executes as:
let trip: Itinerary = ai.Agent<Itinerary>
    .new()
    .run(PlanTrip@spec(trip_request = "2 weeks in Japan"))
    .value;
```

This is the compiler's direct-call desugaring. Use the explicit form when you
need the full `RunResult` rather than only its value.

## One-turn functions

A function without `tools:` runs the same loop and completes on the
first model turn. Extraction and classification are one-turn runs, not
a separate kind of function, so they record the same journal shape and
throw the same errors. `max_steps` is accepted on a toolless call and
is inert, because the loop needs one turn.

## `$` parameters

Configuration shares the call parentheses with the function's
arguments, distinguished by a `$` prefix. A bare name goes to the
function; a `$` name sets the matching field on the default runner:

```baml
let trip: Itinerary = PlanTrip("2 weeks in Japan", $max_steps = 20);
// executes as:
let trip: Itinerary = ai.Agent<Itinerary>
    .new(max_steps = 20)
    .run(PlanTrip@spec(trip_request = "2 weeks in Japan"))
    .value;
```

The catalog:

| Parameter | Runner field | Meaning |
|---|---|---|
| `$client` | `client` | overrides the spec's client for this run |
| `$max_steps` | `max_steps` | model-turn budget, default 12 |
| `$tool_errors` | `tool_errors` | `Report` (default) or `Raise` |
| `$on_event` | `on_event` | observes journal events as they append |

Function parameters cannot start with `$`, so the namespaces cannot
collide. Anything a `$` parameter cannot express uses a runner
explicitly.

## Switching the client

`$client` is the one override:

```baml
let trip: Itinerary = PlanTrip("2 weeks in Japan", $client = fast_client);
```

Running one function across several providers is a loop over client
values:

```baml
let candidates: Client[] = [
    ai.clients.resolve("openai/gpt-5.6"),
    ai.clients.resolve("anthropic/claude-sonnet-5"),
    ai.clients.resolve("google/gemini-2.5-flash"),
];
for (let c in candidates) {
    let trip: Itinerary = PlanTrip("2 weeks in Japan", $client = c);
    score(c.id(), trip);
}
```

There is no second mechanism. `Agent { client: ... }` in the
desugared form is the same setting, and specs carry no rebinding
methods (`../02_specs_and_runners/01_specs.md`).

## There is no `runner:` field

The function block does not name a runner. A function declaration is a
static template that must also work as a plain call, and a runner is
application infrastructure configured where the application runs.
Using a different runner is explicit at the call site:

```baml
let result = my_eval_runner.run(PlanTrip@spec(trip_request = "2 weeks in Japan"));
```

## Step budgets

Every run has a model-turn budget; the default is 12. Exhausting it
throws `StepBudgetExceeded`. Parse repair happens within a turn under
its own attempt budget, so a malformed output does not consume a step.

## Errors at the call site

A direct call throws when the run cannot produce the return type:

- `StepBudgetExceeded` — the budget ran out.
- `ToolFailedError` — a `Raise`-mode tool failed.
- A classified provider failure (`RateLimited`, `NetworkFailure`,
  `InvalidRequest`, `Refused`, `ParseFailed`) — the client's retry
  policy, if any, is already exhausted.
- `baml.errors.UnknownError` — an untyped failure, wrapped.

Handle failure with typed arms:

```baml
let trip: Itinerary = PlanTrip(request) catch_all (e) {
    let b: StepBudgetExceeded => fallback_itinerary(request),
    _ => throw e,
};
```

The full catalog and the conditions that produce each error are in
`../../04_reference/03_errors.md`.

## Every call is recorded

A run records a journal even when you never read it: every model turn,
tool call, and token count, in order. The plain call form discards the
journal with the rest of `RunResult`; to keep it, use the explicit
form:

```baml
let result: RunResult<Itinerary> = ai.Agent<Itinerary>
    .new()
    .run(PlanTrip@spec(trip_request = "2 weeks in Japan"));
let trip: Itinerary = result.value;
inspect(result.journal);
```

`../04_the_journal.md` describes what is recorded.
