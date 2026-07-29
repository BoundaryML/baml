# Streaming, media, and transcription

LLM functions can accept images, PDFs, and finite audio as typed values. Run
the same task with `ai.run.Stream` when the application should receive partial
output before the final typed value.

## Utilities used

| Utility | What it does |
| --- | --- |
| `image`, `pdf`, `audio` | Typed finite media values |
| `ai.run.Stream<TPartial, T>` | Produces partial values and one final `T` |
| `ai.transcription.AudioStream` | A finite sequence of audio chunks |
| `ai.run.Transcribe` | Converts finite audio to text |
| `ai.run.TranscribeWithMeta` | Same transcription task, keeping provider metadata |

## Example

```baml
function InspectImage(value: image) -> string {
  provider: "openai/gpt-5.6-luna"
  prompt: `Describe this image in one short sentence: ${value}`
}

function InspectPdf(value: pdf) -> string {
  provider: "openai/gpt-5.6-luna"
  prompt: `Summarize this PDF: ${value}`
}

let description = InspectImage(
  image.from_url("https://example.com/receipt.png", "image/png"),
)

let summary = InspectPdf(
  pdf.from_url("https://example.com/invoice.pdf", "application/pdf"),
)
```

### Illustrative output

```console
[INFO] InspectImage called with image/png input
[INFO] returned "A crumpled coffee-shop receipt on a wooden table."
[INFO] InspectPdf called with application/pdf input
[INFO] returned "A one-page invoice showing a duplicate charge."
```

The provider adapter chooses the wire representation for each media value.
The LLM function keeps a provider-independent signature: `InspectPdf@task(...)`
renders the PDF structurally into the prompt (as `pdf::url` plus its address),
and the concrete adapter decides how those bytes travel.

## Stream partial output

```baml
class Resolution {
  category: string,
  priority: TicketPriority,
  summary: string,
  reply: string,
}

function ResolveTicket(ticket: SupportTicket) -> Resolution {
  provider: "openai/gpt-5.6-luna"
  prompt: `
    Resolve this support ticket.
    Subject: ${ticket.subject}
    Body: ${ticket.body}

    ${ctx.output_format}
  `
}

function drain_resolution_stream(
  task: ai.Task<Resolution>,
) -> Resolution {
  let stream = task.run(
    runner = ai.run.Stream<Resolution$stream, Resolution>.new(),
  );

  stream.final()
}

let resolution = drain_resolution_stream(ResolveTicket@task(sample_ticket()))
```

### Illustrative output

```console
[INFO] ResolveTicket stream opened
[INFO] partial: summary = "Duplicate charge"
[INFO] partial: reply += "We will investigate order O-42."
[INFO] stream finalized as Resolution
```

`Resolution$stream` is the compiler-projected partial view of the declared
output. It may have unfinished fields while bytes are arriving; the
application can show those partials as they appear. `stream.final()` drains
the provider's response and validates the typed result as the full
`Resolution`. A provider that cannot stream is rejected with
`baml.errors.Unsupported` before any bytes move.

## Transcribe, then run an LLM function

Transcription is a separate bounded operation. This keeps audio transport
settings out of the resolution prompt:

`ai.transcription.TranscriptionProvider` is the portable capability. A
concrete adapter such as `openai.AudioTranscription` owns the provider's audio
encoding, endpoint, response parsing, and usage fields.

```baml
function TranscribeCall(audio: ai.transcription.AudioStream) -> string {
  provider: openai.AudioTranscription {
    inner: openai.Chat { ...live_openai(), model: "gpt-audio" },
    received_chunks: 0,
  }
  prompt: `Transcribe this finite customer-support recording.`
}

function resolve_recorded_call(
  recorded_audio: ai.transcription.AudioStream,
) -> Resolution {
  let transcript = TranscribeCall@task(recorded_audio)
    .run(
      runner = ai.run.Transcribe.new(
        language = "en",
        prompt = "Customer-support call",
      ),
    );

  ResolveTicket(SupportTicket {
    id: "T-100",
    subject: "Recorded call",
    body: transcript,
    customer_tier: "pro",
  })
}

let recorded_audio = ai.transcription.AudioStream {
  chunks: [audio.from_url("https://example.com/call.wav", "audio/wav")],
  sample_rate_hz: 24000,
  channels: 1,
};

let resolution = resolve_recorded_call(recorded_audio)
```

### Illustrative output

```console
[INFO] transcription started: language = "en"
[INFO] transcription completed: 1 chunk, 24000 Hz
[INFO] ResolveTicket started
[INFO] returned Resolution { category: "billing", ... }
```

Transcription engines generally ignore prompt text; the prompt on a
transcription function is advisory vocabulary/context at best, and providers
that cannot use it drop it. The task model is kept anyway so transcription
gets the same routing, override, and runner machinery as every other task.

The task owns the audio argument, the configured `openai.AudioTranscription`
provider, and the function's prompt. `TranscribeCall@task(...)` builds an
`ai.transcription.TranscriptionTask`, so an unrelated `Task<string>` can never
reach a transcription runner. `Transcribe` owns only transcription policy such
as the language and an optional vocabulary-hint override. Like every other
runner, it is selected through `task.run(...)`; callers do not invoke the
runner with raw audio.

Use `TranscribeWithMeta` when you also need the transcription request ID and
usage. It runs the same task and returns `ai.ResponseWithMetadata<string>`. A
finite `AudioStream` ends; a live microphone belongs to an
`ai.realtime.LiveSession` or `ai.run.VoiceAgent`.

## Retry boundary

Before the first partial becomes visible, a safe policy may replay the whole
stream. After the application observes output, replay could duplicate text or
effects. Streaming runners track that boundary so retry policy can fail
closed.
