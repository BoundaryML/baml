# Transcribe audio

Transcription is a specialized bounded provider operation.

## Utilities used

| Utility | Purpose |
| --- | --- |
| `ai.run.Transcribe` | Returns transcript text |
| `ai.run.TranscribeWithMeta` | Returns `Response<string>` |
| `ai.AudioStream` | Finite audio input |

## Example

```baml
class CallSummary {
  customer_request: string,
  promised_action: string?,
}

function SummarizeCall(transcript: string) -> CallSummary {
  provider: "openai/gpt-5.6-luna"
  prompt: `
    Summarize this support call.

    ${transcript}

    ${ctx.output_format}
  `
}

function TranscribeAudio(audio: ai.AudioStream) -> string {
  provider: TranscriptionModel
  prompt: `
    Transcribe this finite customer-support recording.
  `
}

function transcribe_and_summarize(
  audio_stream: ai.AudioStream,
) -> CallSummary {
  let response = TranscribeAudio
    .task(audio_stream)
    .run(
      runner = ai.run.TranscribeWithMeta.new(
        language = "en",
        prompt = "Customer-support call",
      ),
    );

  log.info(response.meta);
  SummarizeCall(response.value)
}
```

## Configuration

| Setting | Default | Meaning |
| --- | --- | --- |
| `language` | Provider default | Language hint |
| `prompt` | The function prompt | Override the vocabulary or context hint |

The LLM function's task owns the provider and finite audio argument. The
transcription provider owns wire encoding and transcript decoding. The runner
owns transcription policy and chooses whether metadata is preserved. It is
always passed to `task.run(runner = ...)`; it is not called directly with an
audio stream.

[Back to media and live sessions](../media-and-live-sessions.md)
