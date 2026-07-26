# Tools during a realtime session

> **Status:** Implemented — the reference uses a real OpenAI realtime session
> with a correlated tool call and result.

Realtime tool use combines two lifecycles: provider-session events and
application tool execution. Call IDs and event order must survive both.

`VoiceSupport` is the `Task<null>` function from
[Realtime interaction with `Channel` and `LiveSession`](./03-realtime-channel.md). It
supplies session instructions and tools; results remain live events rather
than one hidden typed return value.

## Open with an initial tool roster

```baml
let registry = ai.ToolRegistry.new([lookup_order])

let live_session = VoiceSupport.task(customer_id)
  .with_tools(registry.snapshot())
  .run(
    runner = ai.run.Realtime.new(trace_channel),
  )
```

## Dispatch provider tool events

```baml
for (let event in live_session.receive()) {
  match (event) {
    let requested: ai.LiveToolCalls => {
      let results = requested.calls.map((call) -> {
        registry.dispatch(call)
      })
      live_session.submit_tool_results(results)
    },
    _ => handle_live_event(event),
  }
}
```

## Dynamic tools

If the realtime protocol permits tool updates, synchronize a registry snapshot
before the next response. A provider whose session fixes tools at open time
must expose that limitation; the driver should not pretend an update succeeded.

The open `LiveSession` resource owns the provider session and result submission. The
application owns dispatch and must return exactly one correlated result for
every provider-requested call ID.
