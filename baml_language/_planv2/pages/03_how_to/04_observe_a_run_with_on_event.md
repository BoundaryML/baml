# Observe a run with on_event

Set `on_event` on the default runner to react to journal events as they
append: logging, progress reporting, metrics. The callback receives
every event in order, on the run's thread, while the run executes. It
observes; it cannot veto or mutate the run
(`../02_guides/02_specs_and_runners/02_the_default_runner.md`). This
page runs the travel agent:

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

A callback is a plain function over `Event`. This one logs every event
kind as one line:

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

Pass it per call with `$on_event`, or set the runner field in the
explicit form:

```baml
let trip: Itinerary = PlanTrip("2 weeks in Japan", $on_event = log_event);

let result: RunResult<Itinerary> = ai.Agent<Itinerary>
    .new(on_event = log_event)
    .run(PlanTrip@spec(trip_request = "2 weeks in Japan"));
```

Events arrive as the runner appends them: a model turn commits as one
batch, so its `AssistantMessage`, its `ToolRequested` projections, and
its `Usage` fire together, followed by each tool's `ToolCompleted` or
`ToolFailed` in completion order (`../04_reference/02_events.md`). The
callback sees more than the model does, because journal-only events
such as `ToolRequested` and `Usage` fire even though they lower to
nothing in the transcript.

Token deltas are not events in this phase
(`../02_guides/04_the_journal.md`). The Claude Code client additionally
streams its harness's inner transcript as log lines, which is logging
rather than events
(`../02_guides/03_clients/05_the_built_in_clients.md`).
