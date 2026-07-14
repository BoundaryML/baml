# Tools during a realtime session

Realtime tool use combines two lifecycles: provider-session events and
application tool execution. Call IDs and event order must survive both.

## Open with an initial tool roster

```baml
let registry = ai.ToolRegistry.new([lookup_order])

let live = ai.drivers.open_live(
  VoiceSupport.task(customer_id, $provider = RealtimeModel)
    .with_tools(registry.snapshot()),
  audio_channel,
)
defer { live.cleanup() }
```

## Dispatch provider tool events

```baml
for (let event in live.events()) {
  match (event) {
    let requested: ai.LiveToolCalls => {
      let results = requested.calls.map((call) -> {
        registry.dispatch(call)
      })
      live.submit_tool_results(results)
    },
    _ => handle_live_event(event),
  }
}
```

## Dynamic tools

If the realtime protocol permits tool updates, synchronize a registry snapshot
before the next response. A provider whose session fixes tools at open time
must expose that limitation; the driver should not pretend an update succeeded.

## Current design boundary

The BEP defines realtime and tool capabilities separately, but exact
`LiveToolCalls`/`submit_tool_results` signatures still need to be made
normative. This page records the intended ownership and event flow without
claiming finalized names.

## Related design and scenarios

- Scenarios 13 dynamic tools, 24 realtime tools, 39 harness extensibility

