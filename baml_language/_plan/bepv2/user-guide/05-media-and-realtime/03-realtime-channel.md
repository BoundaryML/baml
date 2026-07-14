# Realtime interaction with `Channel` and `Live`

Realtime is an explicit resource lifecycle. Create a task value, supply a
channel, and retain the returned live session.

## Use it

```baml
let task = VoiceSupport.task(
  customer_id,
  $provider = RealtimeModel,
)

let live = ai.drivers.open_live(task, audio_channel)
defer { live.cleanup() }

for (let event in live.events()) {
  match (event) {
    let delta: ai.TranscriptDelta => ui.append(delta.text),
    let closed: ai.LiveClosed => return,
    _ => {},
  }
}
```

## Ownership

```text
Channel: caller-owned input/output plumbing
Live:    provider-session identity, event ordering, controls, cleanup
Task:    instructions, output contract, and selected provider
```

A realtime-only provider implements `RealtimeProvider`, not `DriveProvider`
merely to make `VoiceSupport(...) -> void` compile. Returning `void` does not
define when the session finishes and would hide both required objects.

## Providers supporting both modes

A concrete provider may implement both interfaces if it separately defines a
bounded `drive<T>` policy. Realtime capability alone does not imply that
policy.

## Related design and scenarios

- [Realtime resources](../../pages/06-resources.md#realtime)
- Scenario 22 realtime voice
