# baml_live

A realtime voice demo written in BAML. It holds a spoken, full-duplex
conversation with an OpenAI speech-to-speech model, lets the model call BAML
functions as tools during the call, and returns a typed value when the call
ends.

The package is self-contained. It uses the released standard library
(`baml.ws`, `baml.sys.start_process`, `ai`, `reflect`) and changes none of it.

## Run it

Run these from the repository root. `OPENAI_API_KEY` must be set; in this repo,
`infisical run --env=dev --` provides it.

```bash
infisical run --env=dev -- baml run --project baml_language/crates/baml_tests/baml_live talk
```

| Script | What it does |
| --- | --- |
| `talk` | Talks to the reservation line on this Mac's microphone and speakers, on GPT-Live. Say goodbye to end the call. |
| `talk_realtime` | The same conversation on the OpenAI Realtime API. |
| `mic_check` | Records the microphone for three seconds while playing a phrase. It opens no model session. |
| `selftest` | Needs no microphone. It synthesizes a caller with text to speech, holds the conversation on both providers, and reports what each one did. |

The first `talk` compiles `support/duplex_audio_macos.swift` with `swiftc`,
which takes about 30 seconds. The binary is cached under `$TMPDIR/baml_live`.
macOS asks for microphone permission for the terminal application on first use.
When the helper is not found, set `BAML_LIVE_AUDIO_HELPER` to the path of the
Swift file.

## Tests

```bash
baml test --project baml_language/crates/baml_tests/baml_live
```

```bash
infisical run --env=dev -- baml test --project baml_language/crates/baml_tests/baml_live --profile live
```

The default profile runs offline. It covers the wire messages, the audio
helpers, and a complete `VoiceAgent` run against a scripted fake provider. The
`live` profile holds real conversations on both providers. Its caller is
synthesized with text to speech, and the assistant's recorded audio is
transcribed by a separate speech-to-text model, so the test checks the audio
and not only the provider's own transcript events. Recordings are written to
`$TMPDIR/baml_live/selftest_*.wav`.

## How it works

The demo is one ordinary LLM function. `ReservationLine` in `main.baml` has a
prompt, two tools, and a `CallOutcome` return type. Calling it directly would
run it as text through `ai.Agent`. The demo runs the same spec through
`voice.VoiceAgent` instead:

```baml
let agent = voice.VoiceAgent.new(audio = voice.MacAudioDevice.new());
let outcome = agent.run(ReservationLine@spec("Luigi's")).value;
```

`VoiceAgent` implements `ai.Runner`. It maps the spec onto a speech session as
follows.

| Spec | Session |
| --- | --- |
| `prompt` | The voice model's instructions. |
| `tools` | Function tools the model can call during the conversation. The runner executes them beside the audio stream, so the conversation continues while a tool runs. |
| Return type | The argument of a `finish_conversation` tool that the runner adds. The model calls it when the user says goodbye. The runner checks the argument against the return type and reports a mismatch back to the model, which then corrects the call. |
| `client` | `openai/gpt-live-*` selects `OpenAILive`, and `openai/gpt-realtime*` selects `OpenAIRealtime`. A `provider` argument overrides the selection. |

The run returns an `ai.RunResult`. Its journal holds the transcribed speech as
`UserMessage` and `AssistantMessage` events, the tool calls with their results,
and the final value. `on_event` observes these events as they are recorded, and
`on_voice` observes the raw session events, which the demo uses for captions.

A run ends with `voice.SessionEnded` when the session closes before the model
finished, and with `baml.errors.Timeout` when nobody speaks for
`idle_timeout_ms` or the call exceeds `max_duration_ms`.

### Files

| File | Contents |
| --- | --- |
| `main.baml` | The demo function, its tools, the entry points, and captions. |
| `ns_voice/agent.baml` | `VoiceAgent`, the runner. |
| `ns_voice/session.baml` | `VoiceProvider`, `VoiceSession`, and the `VoiceEvent` union. This is the contract between the runner and a provider. |
| `ns_voice/openai_live.baml` | GPT-Live over `wss://api.openai.com/v1/live/sessions`. |
| `ns_voice/openai_realtime.baml` | The Realtime API over `wss://api.openai.com/v1/realtime`. |
| `ns_voice/audio.baml` | `AudioDevice`, `MacAudioDevice`, `ScriptedAudioDevice`, and `PlaybackClock`. |
| `ns_voice/speech.baml` | Test support: text to speech and speech to text. |
| `support/duplex_audio_macos.swift` | The microphone and speaker helper process. |
| `tests.baml` | The offline and live tests. |

### The two providers

GPT-Live and the Realtime API are different protocols, and `VoiceSession`
hides the differences from the runner.

| | GPT-Live | Realtime |
| --- | --- | --- |
| First message | `session.start`. The configuration is fixed afterwards. | `session.update`. |
| Turn taking | Full duplex. The model listens while it speaks and yields by itself. | Server voice activity detection commits the user's turn and starts a response. |
| Output audio | A continuous stream at real-time pace, with digital silence while the model is quiet. | Bursts that arrive faster than they play. |
| Interruption | Handled by the model. The client does nothing. | The client discards its queued audio and sends `conversation.item.truncate` with the milliseconds the user heard. `PlaybackClock` tracks that position for each assistant item. |
| Tools | The voice model has none. It delegates to a Responses backend model, which owns the function tools. Backend events arrive inside `response.event`. Results return with `response.item.create`, followed by `response.create`. | On the session. Calls arrive in `response.done`. Results return with `conversation.item.create`, followed by `response.create`. |
| Usage | Billed per second. Token usage is reported for the backend only. | Token usage for each response. |

The GPT-Live voice model decides when to delegate, so `OpenAILive` adds the
tool list to its instructions and tells it to hand off once more when the user
says goodbye. Backend replies surface as `voice.BackendReply` events. The
captions print them, which shows what the backend reported and what the voice
model then said.

The Realtime server starts responses by itself, and only one response can be
active. `OpenAIRealtimeSession` therefore defers a `response.create` that would
collide with an active response until `response.done` arrives.

### Audio

All audio on the BAML side is PCM16, 24 kHz, mono. `MacAudioDevice` starts the
Swift helper as a child process and exchanges Base64 frames with it over its
pipes, one frame per line. Capture and playback share one `AVAudioEngine` in
voice-processing mode, so macOS cancels the speaker signal out of the
microphone signal. The microphone stays open while the assistant speaks, and
the assistant does not hear itself. `MacAudioDevice.new(echo_cancellation =
false)` skips voice processing, which suits headphones.

`ScriptedAudioDevice` plays prepared lines as the caller at real-time pace and
records the assistant. A line starts when the assistant has spoken and then
stayed quiet, and a line can additionally wait for a cue. The self-test cues
the caller's thanks on the completion of `book_table`.

## Known limits

- The models do not always follow the instruction to end the call only after
  the user says goodbye. The Realtime model sometimes ends the call directly
  after booking.
- GPT-Live emits transcript fragments without turn boundaries. The journal
  starts a new message when the speaker changes, so overlapping speech produces
  short alternating messages.
- Realtime user transcripts come from a separate transcription model and can
  arrive after the assistant has started to answer.
- `MacAudioDevice` uses the default input and output devices.
