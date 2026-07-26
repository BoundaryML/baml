# Realtime interaction with `Channel` and `LiveSession`

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

let live_session = task.run(
  runner = ai.run.Realtime.new(trace_channel),
)

let closed = false
while (!closed) {
  for (let event in live_session.receive()) {
    match (event) {
      let delta: ai.TranscriptDelta => ui.append(delta.text),
      let ended: ai.LiveClosed => { closed = true },
      _ => {},
    }
  }
}
```

`VoiceSupport` returns `null` because this session does not produce one final
application value. Its task still carries the instructions, arguments, tools,
and selected provider. Text, audio, tool calls, interruptions, and closure are
observed through `LiveEvent` values instead. There is no
`${ctx.output_format}` because there is no final `T` to parse.

## Duplex audio stays small

The audio device and provider session are separate resources. Provider-side
VAD is session configuration; it is not another provider or runner.

```baml
function run_voice_agent(task: ai.Task<null>, audio: ai.RealtimeAudioDevice) -> null {
  let live_session = task.run(
    runner = ai.run.Realtime.new(trace_channel),
  )

  let microphone_pump = spawn {
    pump_microphone(audio, live_session)
  }

  let closed = false
  while (!closed) {
    let events = live_session.receive()
    for (let event in events) {
      match (event) {
        let audio_delta: ai.AssistantAudioDelta => {
          audio.play_output(audio_delta.audio)
        },
        let speech: ai.UserSpeechStarted => {
          let played_ms = audio.stop_output()
          live_session.truncate_assistant_audio(played_ms)
        },
        let tools: ai.LiveToolCalls => {
          live_session.submit_tool_results(dispatch(tools.calls))
        },
        let ended: ai.LiveClosed => {
          microphone_pump.cancel()
          closed = true
        },
        _ => log.info(event),
      }
    }
  }
}
```

`OpenAiRealtime.server_vad` enables the provider's speech boundaries,
automatic response creation, and interruption. A manually delimited recording
can instead use `send_audio_turn(...)`; continuous microphone capture should
use `send_audio(...)`.

## Ownership

```text
Channel:     caller-owned tracing/transport observation
LiveSession: provider-session identity, event ordering, input, and controls
Task:        instructions, arguments, tools, and selected provider
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
but open-ended `LiveSession` should not pretend every session produces one `T`.

## Related design

- [Realtime resources](../specification/07-resources.md#realtime)
- [What happens to `T`](../specification/03-drivers.md#what-happens-to-t)
