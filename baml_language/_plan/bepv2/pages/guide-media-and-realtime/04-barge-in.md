# Handle realtime barge-in

> **Status:** Implemented in the executable reference.

Interruption controls operate on the opened `LiveSession` resource because they must
target one provider session and its audio timeline.

## React to user speech

```baml
for (let event in live_session.receive()) {
  match (event) {
    let speech: ai.UserSpeechStarted => {
      // With server VAD, the provider already interrupts generation.
      let played_ms = audio.stop_output()
      live_session.truncate_assistant_audio(played_ms)
      ui.mark_interrupted(played_ms)
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
not identify which provider response should be cancelled. `LiveSession` owns that
identity and ordering.

## Test it

Use a scripted event source and recording channel. Assert that:

1. `response.cancel` is emitted before truncation;
2. truncation uses the measured playback position; and
3. the next user turn still proceeds.
