# Handle realtime barge-in

Interruption controls operate on the opened `Live` resource because they must
target one provider session and its audio timeline.

## React to user speech

```baml
for (let event in live.events()) {
  match (event) {
    let speech: ai.UserSpeechStarted => {
      live.cancel_response()
      live.truncate_assistant_audio(speech.played_ms)
      ui.mark_interrupted(speech.played_ms)
    },
    let delta: ai.TranscriptDelta => ui.append(delta.text),
    _ => {},
  }
}
```

Cancel before truncating so the provider stops producing the response that is
being shortened. `played_ms` records what the user actually heard, not merely
what the model generated.

## Why this is not a channel method

The channel transports frames and may be reattached or multiplexed. It does
not identify which provider response should be cancelled. `Live` owns that
identity and ordering.

## Test it

Use a scripted event source and recording channel. Assert that:

1. `response.cancel` is emitted before truncation;
2. truncation uses the measured playback position; and
3. the next user turn still proceeds.

## Related design and scenarios

- Scenario 23 barge-in

