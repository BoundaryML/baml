# Open a live session

A raw live session is an explicit provider resource, not a runner.

## Utilities used

| Utility | Purpose |
| --- | --- |
| `ai.open_live` | Opens a `LiveSession` |
| `ai.Channel` | Caller-owned transport observation |
| `ai.LiveEvent` | Text, audio, tool, and lifecycle events |

## Example

```baml
function VoiceSupport(customer_id: string) -> null {
  provider: "openai/gpt-realtime"
  prompt: `
    Help customer ${customer_id} over a live voice session.
  `
}

let session = ai.open_live(
  VoiceSupport.task("customer-7"),
  trace_channel,
);

defer { session.close() }

let closed = false;
while (!closed) {
  for (let event in session.receive()) {
    match (event) {
      let delta: ai.TranscriptDelta => ui.append(delta.text),
      let ended: ai.LiveClosed => { closed = true },
      _ => {},
    }
  }
}
```

`VoiceSupport` returns `null` because the session has no single intrinsic typed
result. The `LiveSession` exposes ongoing input, output, interruption, and
close operations.

Opening the raw session does not automatically execute application tools. A
managed `VoiceAgent` or explicit tool wrapper owns that policy.

[Back to media and live sessions](../media-and-live-sessions.md)
