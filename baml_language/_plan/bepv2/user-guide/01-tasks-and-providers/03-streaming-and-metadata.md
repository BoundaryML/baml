# Streaming and response metadata

Keep the LLM function unchanged and choose a driver matching the information
the application needs.

## A complete value plus metadata

```baml
let response = ai.drivers.drive_with_meta(ResolveTicket.task(ticket))

ui.show(response.value.reply)
metrics.record(response.meta.usage)
log.info(response.meta.request_id ?? "no request id")
```

`Response<T>` keeps provider/model identity, usage, finish reason, attributes,
raw provider data, and an optional exact transcript beside the typed value.
Reasoning summaries and citations belong in metadata or typed model blocks,
not inside `Resolution` unless they are part of the business result.

## Stream partial output

```baml
let stream = ai.drivers.stream(ResolveTicket.task(ticket))

while (true) {
  match (stream.next()) {
    let finished: baml.stream.StreamFinished => break,
    let partial => ui.render_partial(partial),
  }
}

let resolution: Resolution = stream.final()
```

The compiler projects `Resolution` into its partial stream type when the task
flows to `stream`. There is no generated `ResolveTicket.stream` companion.

## What changed

```diff
- let resolution = ResolveTicket(ticket)
+ let response = ai.drivers.drive_with_meta(ResolveTicket.task(ticket))
+ let resolution = response.value
```

or:

```diff
- let resolution = ResolveTicket(ticket)
+ let stream = ai.drivers.stream(ResolveTicket.task(ticket))
+ while (true) {
+   match (stream.next()) {
+     let finished: baml.stream.StreamFinished => break,
+     let partial => ui.render_partial(partial),
+   }
+ }
+ let resolution = stream.final()
```

## Failure boundary

Streaming fallback is safe only before the first observable chunk. Once the UI
has seen output, silently restarting from another provider would duplicate or
contradict visible data.

## Related design and scenarios

- [Driver catalog](../../pages/03-drivers.md)
- Scenario families: 04 streaming, 07 reasoning, 08 enriched outputs, 32 observability, 34 cost and tokens
