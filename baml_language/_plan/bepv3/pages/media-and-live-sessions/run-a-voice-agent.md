# Run a voice agent

`VoiceAgent` is a runner because it owns a complete application lifecycle over
a live provider session.

## Utilities used

| Utility | Purpose |
| --- | --- |
| `ai.run.VoiceAgent` | Owns session loop and audio device lifecycle |
| `RealtimeAudioDevice` | Captures and plays audio |
| `Channel` | Receives provider frames and trace data |

## Example

```baml
function lookup_order(order_id: string) -> string {
  "out for delivery"
}

function VoiceSupport(customer_id: string) -> null {
  provider: "openai/gpt-realtime"
  prompt: `
    Help customer ${customer_id} over a live voice call.
  `
  tools: [lookup_order]
}

VoiceSupport.task("customer-7").run(
  runner = ai.run.VoiceAgent.new(
    audio = audio_device,
    channel = trace_channel,
    barge_in_after_ms = 500,
  ),
)
```

## Configuration

| Setting | Default | Meaning |
| --- | --- | --- |
| `audio` | Required | Microphone and playback device |
| `channel` | Required | Provider channel and trace sink |
| `barge_in_after_ms` | `500` | Sustained speech before interruption |
| `completion_tool` | None | Optional tool that ends the call |

The runner pumps microphone input beside provider events, executes application
tools, handles barge-in, and closes both session and audio device on every exit
path.

[Back to media and live sessions](../media-and-live-sessions.md)
