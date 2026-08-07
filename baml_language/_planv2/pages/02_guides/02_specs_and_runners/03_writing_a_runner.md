# Writing a runner

## The `Runner` interface

```baml
interface Runner<Out> {
    type Output
    type Error
    function run(self, spec: FunctionSpec<Out>) -> Self.Output throws Self.Error
}
```

The associated types are why the interface exists. Different kinds of
run return different things — the default runner returns a
`RunResult`, an eval runner returns scores, a future background runner
returns a pollable handle — and fail in different ways, so each
implementation declares its own `Output` and its own `Error` union.
The interface never throws an untyped error: a caller matches the
concrete runner's declared errors with typed arms. The set of runners
is open; a new kind of run is a new implementation, not a language
change.

The interface requires no fields. A BAML interface can oblige its
implementors to carry properties, but no run option is universal:
`max_steps` means nothing on a runner that delegates its loop, and
`client` and `on_event` are ambiguous on a runner that owns several
runs. A required field that an implementation ignores is a false
promise, so the interface stays at the two associated types and `run`.
Shared options come from composition instead: a runner that drives the
standard loop embeds an `Agent` and passes it through, which keeps
`max_steps`, `tool_errors`, and `on_event` under their existing names.

## The building blocks

The default runner is built from the same public primitives any runner
uses; there are no intermediate loop helpers to learn:

- `Journal.new(spec)` — a journal with `RunStarted` appended;
  `append_all` is the write.
- `client.invoke(ModelTurnInput { ... })` — one model turn, from
  materials the runner assembles, which is also where a custom toolbox
  goes.
- `Tool.call(args)` — validated dispatch through
  `reflect.call_any`.
- `baml.sap.parse<Out>(candidate)` — the schema-aware final parse.

The parse-feedback recipe is the worked example of composing them
(`../../03_how_to/01_retry_a_failed_parse_with_feedback.md`). A runner
that drives its own loop upholds the invariants below itself.

## Example: a wrapping runner

A runner that wraps another runner adds behavior without touching the
loop:

```baml
class WithRetries<Out> {
    inner: Runner<Out>,
    attempts: int,

    function _attempt(self, spec: FunctionSpec<Out>, remaining: int) -> Self.inner.Output
        throws Self.inner.Error {
        self.inner.run(spec) catch_all (e) {
            let f: Failure => {
                // replay only while budget remains and the failure is classified Safe
                if (remaining > 1 && f.retry_safety() == RetrySafety.Safe) {
                    self._attempt(spec, remaining - 1)
                } else {
                    throw e
                }
            },
            _ => throw e,    // untyped failures propagate
        }
    }

    implements Runner<Out> {
        type Output = Self.inner.Output
        type Error = Self.inner.Error
        function run(self, spec: FunctionSpec<Out>) -> Self.Output {
            self._attempt(spec, self.attempts)
        }
    }
}
```

The wrapper retries whole runs and only when the failure reports
`Safe`. Model-turn retries belong to the client layer
(`../03_clients/04_reliability.md`); the two do not overlap.

## Example: an eval runner

A runner whose output is not the function's value:

```baml
class CompareClients {
    candidates: Client[],
    agent: Agent<Itinerary>,     // the per-run knobs, under their usual names

    implements Runner<Itinerary> {
        type Output = map<string, RunResult<Itinerary>>
        type Error = Failure | baml.errors.UnknownError
        function run(self, spec: FunctionSpec<Itinerary>) -> map<string, RunResult<Itinerary>> {
            let results: map<string, RunResult<Itinerary>> = {};
            for (let c in self.candidates) {
                // same knobs each time; only the client differs
                let r = Agent { ...self.agent, client: c }.run(spec);
                results.set(c.id(), r);    // each run has its own journal
            }
            results
        }
    }
}
```

```baml
let by_client = CompareClients { candidates: [openai, anthropic, google] }
    .run(PlanTrip@spec(trip_request = "2 weeks in Japan"));
```

The spec is run three times; each run has its own journal.

Task-oriented recipes built from these primitives are collected in
`../../03_how_to/readme.md`.

## What a runner must uphold

A runner that drives the loop itself, rather than delegating to
`Agent` or the building blocks, must keep the invariants the rest of
the system assumes:

- A model turn commits atomically. A failed `invoke` appends nothing.
- Every `ToolUse` id receives exactly one correlated result before the
  next model turn.
- Journal events are appended in order and never rewritten.
- A tool failure is journaled before any exception propagates.
- Only the runner writes the journal; clients and tools do not.
