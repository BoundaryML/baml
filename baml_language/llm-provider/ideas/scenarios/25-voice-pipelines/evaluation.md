# Evaluation — Cascaded voice pipelines + unified voice abstraction

Adversarial read: does "Provider base + capability interfaces + client-as-sugar" actually express a Pipecat-style cascaded voice agent *and* a Mastra-style unified voice object? Where does it hold, where does it leak?

## (a) What the proposal reuses unchanged

- **`Realtime` as the face of a voice session.** `CascadedVoice` and `NovaSonic` both `implements Realtime { run(prompt, io: Channel) -> Transcript }` — the exact pass-in `Channel` model from §3/§5. The app's `VoiceChat.live(system, io)` companion `match`es `Realtime` and routes to `run` with **no knowledge** of whether the implementation is a three-hop cascade or a one-hop STS. The cascaded↔STS architecture fork (the central voice decision) collapses to a one-line client swap. This is the proposal's best moment.

- **Combinators as plain classes that present a richer interface than their members.** `CascadedVoice` is exactly the `Fallback` pattern from §6: a plain non-generic class whose members are `Provider`s, which itself `implements` `Realtime`. STT/LLM/TTS go in as fields; one `Realtime` comes out. The "stages are providers, the wiring is a combinator" framing is native, not bolted on.

- **`requires Provider` makes single-capability stages first-class.** An STT box (`Deepgram`) is a `Provider` whose `call<T>` = "transcribe one clip"; a TTS box is a `Provider` whose `call<T>` = "synthesize" (rarely used). They drop into the combinator as `SttProvider` / `TtsProvider`-typed fields, and `llm: Provider` accepts *any* chat model — so Deepgram × Llama × Cartesia type-checks with no per-combination glue declared in BAML.

- **Swap = field swap.** `PhoneAgent` → `PhoneAgentBudget` swaps all three stages and the combinator body is untouched, because it only ever sees the capability interfaces. Portability is real here.

- **Runtime capability `match` + degrade-or-error.** `CompositeVoice.call` errors (no LLM); `Whisper.listen` degrades streaming to a single buffered final frame. Both are the §5 split — delivery refinement degrades, missing interaction shape errors — applied to audio without new mechanism.

## (b) Net-new surface it must add (each tied to a primitive)

- **`SttProvider` / `TtsProvider` capability interfaces** (the unified `listen`/ `speak` verbs). These are *new capabilities* in the §1 sense — `requires Provider`, single method each. Cheap and on-pattern, but they ARE additions: the proposal ships `Streaming`/`Realtime`/`Tools`/`Inspectable` and nothing audio.

- **Audio media primitives at the host boundary:** `AudioChunk`, `AudioStream` (the audio analogue of `baml.http.SseStream`), `Transcription`, `VoiceTranscript` /`VoiceTurn`. These are `$rust_type`-backed opaque values, mirroring `baml.llm.PromptAst`. Needed because the spine only models text/JSON bodies.

- **`baml.llm.Stream<Transcription, string>` as the STT return.** Reuses the existing `Stream<TStream, TFinal>` shape (partials + final) — no new type, but a new *use* of it (the stream item is `Transcription`, not an output token).

- **A turn-detection host primitive: `VadDetector` + `silero_vad()`**, plus the cancel/queue helpers (`baml.voice.cancel_token`, `block_until_closed`). Nothing in the proposal models VAD/barge-in; it is genuinely new and lives inside the combinator body.

- **Audio members on the realtime event unions:** `InEventAudio`, `OutEventAudio`, `OutEventFlush`. The canonical `Channel` references `InEvent`/`OutEvent` but leaves them abstract; voice needs concrete audio/flush frames. Ties to OQ2 (how the channel surfaces) and to the `Channel` interface itself.

## (c) Where it is awkward, leaky, or unresolved

- **OQ1 (does every capability `requires Provider`?) bites twice.** `CascadedVoice` must supply a `call<T>` even though "one synchronous voice turn" has no honest meaning without a live channel — we degrade it to *just the LLM stage*, dropping STT and TTS entirely. Worse, `CompositeVoice` has **no model at all**, so its forced `call<T>` can only `throw`. Two of the three combinators here have a `call` that is a lie or a stub. This is direct evidence that some capabilities (`SttProvider`, `TtsProvider`, `Realtime`-as-cascade) should be allowed to stand alone rather than refine `call`. The proposal's "best-effort single turn" stance is strained: for a pure I/O adapter there is no turn to be best-effort about.

- **OQ2 (how the live `io` surfaces) + the audio-frame typing gap.** `Channel` is one interface with abstract `InEvent`/`OutEvent`. A telephony G.711 transport, a WebRTC transport, and a raw-WS transport all carry *different* concrete frame types, but `CascadedVoice.run` `match`es `InEventAudio` against a single `Channel`. There is no compile-time guarantee the handed-in channel actually speaks audio — a text `Channel` would simply never match `InEventAudio` and the agent would sit mute. The proposal gives no typed-channel mechanism; transport capability is unchecked.

- **No compile-time guarantee that a client is voice-capable.** Because `client` returns the existential `Provider`, `VoiceChat.live` only discovers at runtime whether the provider is `Realtime`. For `PhoneAgent` it is; but nothing stops an app author wiring a non-realtime client into `VoiceChat` and getting a runtime `Unsupported`. The escape hatch (drop the sugar, write `-> SomeRealtimeProvider`) works but forfeits the `client` ergonomics the scenario is selling.

- **Barge-in / latency budget live entirely in one combinator body, untyped.** The hard parts the background calls out — VAD firing, cancelling in-flight TTS, flushing the speaker queue, stitching STT+TTS text onto one LLM clock — are all imperative code inside `CascadedVoice.run` (`speaking.cancel()`, `OutEventFlush`, `context.append_*`). The model neither helps nor hinders here; it's "just BAML", which means the <300ms budget and echo handling are entirely the author's, with no abstraction the proposal contributes. Honest, but it shows the model stops at the combinator boundary.

- **Server-authoritative STS state is invisible.** For `NovaSonic`, turn-taking, interruption, and conversation state live *on the model's server*. The BAML `run` body is a single `$rust_io_function`; none of that state is expressible or inspectable in the model. The proposal can *invoke* an STS provider but cannot *describe* its session semantics — fine for a one-hop call, but it means the cascaded and STS paths are only superficially interchangeable (the cascade exposes per-turn hooks the STS path cannot).

- **`TtsProvider`-level fallback doesn't compose cleanly.** `with_retry`/ `fallback_to` are `Provider` default methods returning `Retry`/`Fallback`, which forward `call`/`stream`/`run` — but **not** `speak`/`listen`. A `Fallback` of two TTS providers would not forward `TtsProvider.speak` unless `Fallback` is extended to `implements TtsProvider`. So the new capabilities don't get combinator forwarding for free; every combinator must be taught each new verb (the §6 "statically claims every capability it forwards" cost, now multiplied by audio).

## (d) Verdict: **Workable-with-additions**

The core claim survives the voice scenario well: STT/LLM/TTS as composable single-capability providers wired by a `Realtime`-implementing combinator is a *natural* fit for "Provider base + capability interfaces + combinators," and the cascaded↔STS fork genuinely reduces to a one-line client swap behind one `.live(args, io)` companion — the strongest possible endorsement of the unified `Realtime` face. But it is not Clean: it requires a real net-new layer (two voice capability interfaces, audio media primitives, a VAD host primitive, audio event frames), and it exposes two structural seams the proposal already flagged. OQ1 is the sharpest — forcing `call<T>` onto `CompositeVoice` and `CascadedVoice` yields a throw and a half-truth, which is concrete evidence that pure-I/O capabilities want to stand alone. And the absence of typed channels / compile-time realtime guarantees means transport-audio compatibility and "is this client even voice?" are both runtime gambles. The model *expresses* cascaded voice; it does not *help* with the budget, barge-in, or correctness that make voice hard — that all falls into one untyped combinator body. Workable, with the additions above and an honest answer to OQ1.
