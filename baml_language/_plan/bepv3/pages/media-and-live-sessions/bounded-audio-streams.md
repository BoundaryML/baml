# Bounded audio streams

An `AudioStream` is finite input. It ends and may therefore feed a bounded
operation.

## Utilities used

| Utility | Purpose |
| --- | --- |
| `ai.AudioStream` | Finite sequence of audio chunks |
| `ai.run.Transcribe` | Produces text after the stream ends |

## Example

```baml
class CallSummary {
  issue: string,
  next_step: string,
}

function SummarizeCall(transcript: string) -> CallSummary {
  provider: "openai/gpt-5.6-luna"
  prompt: `
    Summarize this completed support call.

    ${transcript}

    ${ctx.output_format}
  `
}

function TranscribeAudio(audio: ai.AudioStream) -> string {
  provider: TranscriptionModel
  prompt: `
    Transcribe this finite recording.
  `
}

function summarize_recorded_call(
  recorded_audio_stream: ai.AudioStream,
) -> CallSummary {
  let transcript = TranscribeAudio
    .task(recorded_audio_stream)
    .run(
      runner = ai.run.Transcribe.new(),
    );

  SummarizeCall(transcript)
}
```

`AudioStream` does not mean an endless duplex session. Realtime audio uses a
`LiveSession` plus a caller-owned audio device or channel.

[Back to media and live sessions](../media-and-live-sessions.md)
