# Bounded `AudioStream` input

> **Status:** Implemented in the executable reference.

An audio stream can be a useful task argument without implying a realtime
conversation. The natural bounded completion event is end-of-stream.

`AudioStream` is finite input for `TranscriptionProvider`; it is not encoded as
chunk counts in an LLM prompt.

## Desired use

```baml
let transcript = ai.drivers.transcribe(
  StreamingTranscriber,
  recording_stream,
  ai.TranscriptionOptions { language: "en" },
)
```

The provider consumes frames until EOF, finalizes decoding, and returns one
transcript. The specialized driver gives the operation a bounded completion
rule without pretending audio transcription is prompt-shaped generation.

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

## Related design


- See [realtime channel](./03-realtime-channel.md) for duplex interaction.
