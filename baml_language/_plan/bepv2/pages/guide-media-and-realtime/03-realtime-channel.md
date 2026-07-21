# Realtime interaction with `Channel` and `Live`

> **Status:** Implemented in the executable reference.

Realtime is an explicit resource lifecycle. Create a task value, supply a
channel, and retain the returned live session.

## Use it

```baml
function VoiceSupport(customer_id: string) -> null {
  provider: RealtimeModel
  prompt: `
    Help customer ${customer_id} over a live voice session.
    Ask for clarification when needed and explain each next step.
  `
}

let task = VoiceSupport.task(customer_id)

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

`VoiceSupport` returns `null` because this session does not produce one final
application value. Its task still carries the instructions, arguments, tools,
and selected provider. Text, audio, tool calls, interruptions, and closure are
observed through `LiveEvent` values instead. There is no
`${ctx.output_format}` because there is no final `T` to parse.

## Ownership

```text
Channel: caller-owned input/output plumbing
Live:    provider-session identity, event ordering, controls, cleanup
Task:    instructions, arguments, tools, and selected provider
```

A realtime-only provider implements `RealtimeProvider`, not `DriveProvider`
merely to make `VoiceSupport(...) -> null` compile. Returning `null` does not
define when the session finishes and would hide both required objects.

## Providers supporting both modes

A concrete provider may implement both interfaces if it separately defines a
bounded `drive<T>` policy. Realtime capability alone does not imply that
policy.

## When you need a typed result

Run a separate typed LLM function over the conversation your application
collected from live events:

```baml
function ResolveCall(conversation: ai.Conversation) -> Resolution {
  provider: SupportModel
  prompt: `
    Resolve this support call from its completed conversation:
    ${conversation}
    ${ctx.output_format}
  `
}

let resolution = ResolveCall(collected_conversation)
```

That gives the typed result a clear completion boundary. A future bounded
realtime API may expose a separate `LiveRun<T>` with `final() -> Response<T>`,
but open-ended `Live` should not pretend every session produces one `T`.

## Related design

- [Realtime resources](../specification/07-resources.md#realtime)
- [What happens to `T`](../specification/03-drivers.md#what-happens-to-t)
