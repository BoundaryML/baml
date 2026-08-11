# The journal

## What the journal is

Every run appends typed events to a journal, in order, from
`RunStarted` to `FinalProduced`. The journal serves two roles with one
structure. During the run it is the transcript source: each model turn
renders from it. After the run it is the trace: `RunResult.journal`
holds the complete record.

The journal is append-only, and the driving runner is the only writer:
clients and tools never append. `Journal.append_all(events)` is the
write, and events are never rewritten, so what a client rendered on
turn three is derivable from the journal at turn three.

The journal is the complete record. Repair attempts, correction
requests, and failures are ordinary events, committed like everything
else — there is no side channel that shows the model something without
recording it. What varies is rendering, not the record: a journal-only
event such as `ToolRequested` lowers to nothing, and later features
filter the transcript the same way
(`../03_how_to/01_retry_a_failed_parse_with_feedback.md`).

## Built-in events

| Event | Fields | Appended |
|---|---|---|
| `RunStarted` | `spec_name`, `arguments` | once, before the first turn |
| `UserMessage` | `content` | by custom runners and committed parse-repair re-asks |
| `AssistantMessage` | `content: Block[]`, `client_id` | one per model turn |
| `ToolRequested` | `id`, `name`, `args` | one per `ToolUse` block, with its turn |
| `ToolCompleted` | `id`, `output` | when the tool returns |
| `ToolFailed` | `id`, `message` | when the tool throws or validation fails |
| `Usage` | `input_tokens`, `output_tokens`, `cached_input_tokens?`, `reasoning_tokens?` | one per model turn |
| `FinalProduced` | `value_json: string` | once, when the final candidate parses |

The reference catalog with ordering and rendering rules is
`../04_reference/02_events.md`.

## `AssistantMessage`

An assistant turn stores structured content — `Text`, `Reasoning`, and
`ToolUse` blocks — plus the id of the client that produced it. The
structure is what makes two things work. Tool results correlate to
calls by id, so a client can lower a result without duplicating the
call. And a turn produced by one client renders under another, because
canonical blocks are client-neutral; `client_id` records provenance,
not a rendering dependency.

`ToolRequested` is a projection of a `ToolUse` block, appended so that
observers and tooling can react to calls without parsing assistant
content. It lowers to nothing in the transcript.

## Reading a run

```baml
let result: RunResult<Itinerary> = ai.Agent<Itinerary>
    .new()
    .run(PlanTrip@spec(trip_request = "2 weeks in Japan"));

for (let e in result.journal.entries()) {
    match (e) {
        let a: AssistantMessage => log.info(`turn by ${a.client_id}`),
        let t: ToolFailed => log.warn(`${t.id}: ${t.message}`),
        _ => null,
    }
}
```

`result.usage` is the sum of the run's `Usage` events. Cost accounting
uses these API-reported numbers, not estimates from prompt sizes.

## What is not recorded

Token deltas are not events. Streaming travels through the pull-based stream
returned by `$stream`; the journal records terminal turns only, so replay does
not become re-streaming. A streamed and blocking turn therefore commit the
same canonical event shape.

Raw HTTP envelopes are not events. A response body is not valid model
input and does not belong in the transcript source. The narrow
API-native data that exact same-provider replay needs arrives in a
later phase as an optional field on the assistant entry
(`../05_appendix/03_future_phases.md`); observability systems that
want full wire traffic capture it at the transport layer.
