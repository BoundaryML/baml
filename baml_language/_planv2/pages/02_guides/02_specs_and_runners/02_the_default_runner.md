# The default runner

## The `Agent` runner

`ai.Agent<Out>` is the runner a plain call uses. Its fields are
the run's configuration, and `$` parameters at a call site set them
(`../01_functions/03_calling_functions.md`):

```baml
class Agent<Out> {
    max_steps: int,                                  // model-turn budget
    client: Client?,                                 // overrides the spec's client when set
    tool_errors: ToolErrorMode,                      // Report or Raise
    on_event: ((Event) -> null throws never)?,       // observes events as they append

    function new(
        max_steps: int = 12,
        client: Client? = null,
        tool_errors: ToolErrorMode = ToolErrorMode.Report,
        on_event: ((Event) -> null throws never)? = null,
    ) -> Agent<Out> throws never

    implements Runner<Out> {
        type Output = RunResult<Out>
        type Error = Failure | baml.errors.UnknownError
        function run(self, spec: FunctionSpec<Out>) -> RunResult<Out>
            throws Failure | baml.errors.UnknownError
    }
}
```

Construct it with `new`; every parameter has a default:

```baml
let runner = ai.Agent<Itinerary>.new(max_steps = 20);
let result: RunResult<Itinerary> = runner.run(PlanTrip@spec(trip_request = "2 weeks in Japan"));
```

## The turn loop, step by step

`run` appends `RunStarted` and iterates:

1. Select the client: the runner's `client` field if set, otherwise
   `spec.default_client`.
2. Check the budget. If `max_steps` model turns have completed, throw
   `StepBudgetExceeded`.
3. Assemble a `ModelTurnInput` from the spec's prompt, the journal,
   the toolbox, and the output type, and call `client.invoke`.
4. Commit the returned turn to the journal as one batch:
   `AssistantMessage`, one `ToolRequested` per `ToolUse` block, and
   `Usage`. A turn that throws commits nothing.
5. If the turn requested tools, execute them concurrently, append one
   `ToolCompleted` or `ToolFailed` per call, and continue at step 2.
   A `Raise`-mode failure throws `ToolFailedError` after its
   `ToolFailed` event is appended.
6. If the turn produced a final candidate, parse it as `Out`. On
   success, append `FinalProduced` and return. Parse repair happens
   within the turn under a fixed budget of two re-asks per step; a
   turn whose candidate cannot be repaired fails with `ParseFailed`.

## The correlation invariant

Every `ToolUse` id in an assistant turn receives exactly one
`ToolCompleted` or `ToolFailed` before the next model turn. The runner
enforces this on the journal, so it holds for every client and does
not depend on adapter code. Results may complete in any order;
correlation is by id, not position.

## Final parsing

The client normalizes and the runner parses. Whatever mechanism
carried the model's answer on the wire, the client surfaces it as the
turn's final candidate — a terminal `Text` block — with
`stop_reason: Complete`. The runner runs the schema-aware parser
(`baml.sap.parse<Out>`) on the candidate. The runner never learns
which wire mechanism carried the value, and the client never touches
`Out`.

A turn is accepted when it requests tool calls or its candidate
parses. When the candidate does not parse and repair cannot fix it,
the runner re-asks within the same step: it commits the failed turn
and a `UserMessage` asking for a correction as ordinary events, then
invokes again, under a fixed budget of two re-asks per step. The
journal is the complete record — every attempt, its usage, and the
correction request are events — but a repair attempt does not consume
a step, and each attempt's token usage counts toward
`RunResult.usage`. A turn whose last attempt still fails throws
`ParseFailed`, with the whole exchange on the record.
A custom runner can write the same loop with its own feedback wording,
because the pieces are public primitives — `spec.prompt()`,
`Journal.append_all`, `UserMessage`, and `client.invoke`
(`../../03_how_to/01_retry_a_failed_parse_with_feedback.md`).

## `RunResult`

```baml
class RunResult<Out> {
    value: Out,          // the parsed final output
    journal: Journal,    // the complete typed record of the run
    usage: Usage,        // aggregated across every model turn
}
```

The plain call form unwraps `value` and discards the rest. Use the
explicit form to keep the journal (`../04_the_journal.md`).

## Observing a run

`on_event` receives every event as it appends, in order, on the run's
thread:

```baml
let result: RunResult<Itinerary> = ai.Agent<Itinerary>
    .new(on_event = (e: Event) -> {
        match (e) {
            let t: ToolRequested => log.info(`tool: ${t.name}`),
            _ => null,
        }
    })
    .run(spec);
```

The callback observes; it cannot veto or mutate. Token deltas are not
events and are not observable in this phase
(`../../05_appendix/03_future_phases.md`).
