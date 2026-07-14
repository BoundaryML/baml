# Bounded `AudioStream` input

An audio stream can be a useful task argument without implying a realtime
conversation. The natural bounded completion event is end-of-stream.

> **Design status:** `AudioStream` is a guide-level proposal; its exact
> normative interface is not yet defined by BEP-064.

## Desired use

```baml
function TranscribeMeeting(audio: ai.AudioStream) -> MeetingTranscript {
  provider: StreamingTranscriber
  prompt: `Transcribe this meeting. ${ctx.output_format}`
}

let transcript = TranscribeMeeting(recording_stream)
```

The provider consumes frames until EOF, finalizes decoding, and returns one
`MeetingTranscript`. That gives `drive<T>` a bounded completion rule.

## Resource properties

The task/runtime should know whether an input is:

```text
AudioFile              serializable and replayable
BufferedAudioStream    process-local but replayable
MicrophoneStream       process-local and single-use
```

A task containing a single-use stream should reject automatic retry,
background serialization, or reuse after consumption unless the application
buffers it explicitly.

## Not a realtime channel

An input stream does not supply speaker output, event handling, barge-in,
response cancellation, multiple turns, or live cleanup. Those belong to
`Channel` plus the `Live` resource.

```text
AudioStream answers: how does bounded input arrive?
open_live answers:   who owns an interactive duplex lifecycle?
```

## Related design and scenarios

- Scenario 25 voice pipelines
- See [realtime channel](./03-realtime-channel.md) for duplex interaction.

