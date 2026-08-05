# Example: background jobs

A job is a task that runs detached: you submit it, get a handle back
immediately, and collect the result later — from the same process or a
different one. Use a job when nobody is waiting on the connection: nightly
reports, bulk enrichment, long research runs.

## Starting a job

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

//# one Background runner per application — it owns the store and worker pool
let jobs = baml.session.Background { store: pg_store, workers: baml.spawn.TaskGroup.new(8) };

let job: Job<Itinerary> = PlanTrip@session(
    trip_request = "3 weeks across South America",
    $runner = jobs,
    $id = "trip-9421",
);   // returns immediately
```

The `$runner` parameter selects the kind of run and changes the handle
type: with `Background`, the expression returns `Job<Itinerary>` instead
of `Session<Itinerary>` (`../02_guides/03_configuration.md`). The job
runs under the runner's journal store, so it survives the process that
started it. Job IDs are create-only by design: starting a job with an
existing ID throws `baml.session.InstanceExists`, so a crashed submitter
that retries cannot start the work twice.

## Polling

```baml
match (job.poll()) {
    let q: baml.session.Queued    => print("waiting for a worker"),
    let r: baml.session.Running   => print(`running, ${r.steps} steps so far`),
    let d: baml.session.Done<Itinerary> => print(d.result.destination),
    let f: baml.session.Failed    => print(f.error),
    let a: baml.session.Aborted   => print("cancelled"),
}
```

`poll()` never blocks. `job.result()` blocks until settlement and returns
`Itinerary` or throws. `job.cancel(reason)` requests an abort through the
control lane.

Re-attach from anywhere with access to the runner's store:

```baml
let job = jobs.attach<Itinerary>("trip-9421");
```

## From another service

```python
from baml_sdk import b

job = b.job.PlanTrip.start(id="trip-9421", trip_request="3 weeks across South America")
# ... later, possibly another process:
job = b.job.PlanTrip.attach("trip-9421")
status = job.poll()
if status.is_done:
    print(status.result.destination)
```

Over HTTP (`baml serve`), starting a job returns `202` with the job ID;
`GET /v1/job/PlanTrip/{id}` returns the status; the journal tail endpoint
streams progress like any session.

## Job vs. task vs. session

The three are the same machinery with different lifetimes and lanes:

| | Returns | Input after start | Backed by |
|---|---|---|---|
| Task: `PlanTrip(...)` | `Itinerary`, blocking | none | in-memory journal |
| Job: `@session(..., $runner = jobs)` | `Job<Itinerary>`, immediate | control lane only (`cancel`) | journal store |
| Session: `PlanTrip@session(...)` | `Session<Itinerary>` | both lanes (`send`, `interrupt`) | memory or store |

A job is a task with a handle — or equivalently, a session without a
`send` lane. The model must drive to `Done` on its own; if it stops with
a conversational reply, that is a `Failed` settlement (`needs_input`),
because nobody is there to answer. If the work might need a human
mid-flight, use a named session instead; it is the same journal either
way, and a UI can always attach to a job's tail to watch it.

## Provider-side background execution

Some providers run long requests in the background on their side (for
example, OpenAI background responses). A client may use that as a
transport detail for a job's model calls — submitting, then polling the
provider — with the usual client obligation: the journal stays the source
of truth, and the provider-side handle is never the state. Whether a
model call runs synchronously or in provider-background mode is invisible
in the journal beyond timing.

## Progress from inside the job

Tools report progress with ambient emits, and the tail shows it live:

```baml
/// Enrich one batch of records.
function enrich_batch(batch_id: string) -> string {
    baml.session.emit(Progress { done: batch_id });
    // ...
    "ok"
}
```

Because jobs settle (see `../02_guides/12_durability.md`), a crashed
worker resumes the job from its last committed batch: completed steps and
tool results are read from the journal, unfinished work re-runs.
