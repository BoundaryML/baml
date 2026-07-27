# Handle barge-in

Barge-in stops assistant playback when the provider reports sustained user
speech.

## Utilities used

| Utility | Purpose |
| --- | --- |
| `UserSpeechStarted` | Provider VAD detected speech |
| `UserSpeechStopped` | Speech ended |
| `truncate_assistant_audio` | Aligns provider history with played audio |

## Example

```baml
function VoiceSupport(customer_id: string) -> null {
  provider: "openai/gpt-realtime"
  prompt: `
    Help customer ${customer_id}. Let the customer interrupt naturally.
  `
}

VoiceSupport.task("customer-7").run(
  runner = ai.run.VoiceAgent.new(
    audio = audio_device,
    channel = trace_channel,
    barge_in_after_ms = 500,
  ),
)
```

The runner waits for sustained speech instead of reacting to every brief VAD
detection. When the threshold is reached it:

1. Stops queued playback.
2. Cancels the active provider response.
3. Measures how much audio actually played.
4. Truncates that exact assistant item.
5. Continues listening to the user.

Truncation never exceeds the audio played for the matching provider item.
Newer assistant audio replaces queued older audio.

[Back to media and live sessions](../media-and-live-sessions.md)
