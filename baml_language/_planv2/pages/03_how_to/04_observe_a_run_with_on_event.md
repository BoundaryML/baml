# Observe a run with on_event

This page reacts to a run's events while the run executes: logging
progress, feeding a trace viewer, counting tokens.

Every run appends typed events to its journal
(`../02_guides/04_the_journal.md`). The default runner's `on_event`
field is a callback that receives each event at the moment it is
appended. The callback observes the run; it cannot change it
(`../02_guides/02_specs_and_runners/02_the_default_runner.md`).

This page runs the travel agent:

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

A callback is a plain function over `Event`. Match the events you care
about; this one writes one log line per event:

```baml
function log_event(e: Event) -> null {
    match (e) {
        let r: RunStarted => log.info(`run started: ${r.spec_name}`),
        let t: ToolRequested => log.info(`tool requested: ${t.name} id=${t.id}`),
        let done: ToolCompleted => log.info(`tool completed: id=${done.id} -> ${done.output}`),
        let failed: ToolFailed => log.info(`tool FAILED: id=${failed.id} ${failed.message}`),
        let u: Usage => log.info(`usage: in=${u.input_tokens} out=${u.output_tokens}`),
        _ => null,
    };
    null
}
```

Pass the callback at the call site with `$on_event`, or set the field
when constructing the runner:

```baml
let trip: Itinerary = PlanTrip("2 weeks in Japan", $on_event = log_event);

let result: RunResult<Itinerary> = ai.Agent<Itinerary>
    .new(on_event = log_event)
    .run(PlanTrip@spec(trip_request = "2 weeks in Japan"));
```

Events arrive in the order the runner appends them. A model turn
arrives as one batch: the `AssistantMessage`, one `ToolRequested` per
tool call, and the turn's `Usage`. Each tool's `ToolCompleted` or
`ToolFailed` follows as that tool finishes. The full catalog of events
and their fields is `../04_reference/02_events.md`.

The callback sees more than the model does. `ToolRequested` and
`Usage` are never rendered into the model's input, but they still
fire, so an observer needs no other channel. Streaming token deltas
are not events in this phase (`../02_guides/04_the_journal.md`).
