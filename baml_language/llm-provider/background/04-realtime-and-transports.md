# 04 — Live conversations

> When the interaction stops being "send a request, read a response" and becomes a
> **living connection**: audio flowing in while audio flows out, the server deciding
> on its own when to speak, the user cutting the model off mid-sentence. This file
> maps realtime/voice, the events that drive it, and the **transport taxonomy**
> underneath everything — from a single HTTP POST up to WebRTC media tracks — and
> the one fault line that forces a use case across the gap.

Single-turn calls are in `01-single-turn.md`; the tool loop is in
`02-tools-and-agents.md`; where conversation state lives (client / server-session /
server-stored) is in `03-state-sessions-memory.md`. This file is about the **wire**:
how bytes move, in which direction, and when a request/response shape stops being
able to describe the interaction at all.

Legend: ★ table-stakes · ◆ advanced · ▲ frontier.

---

## ◆ Realtime / voice — a persistent bidirectional connection

**Goal:** "I want to hold a spoken conversation with the model: stream the user's
microphone up, hear the model's voice stream back, and have it feel like a phone
call — no perceptible turn-taking lag."

### How it's done today

A realtime session is **not** a request/response call. The client opens one
long-lived connection (WebSocket or WebRTC — see the taxonomy below) and from then
on **both sides push typed JSON events** independently. There is no `create()` that
returns a response object; there is `connect()` that returns an open channel.

The event grammar is the heart of it. Conceptually the server holds three things:

- a **server-side conversation** — an ordered list of *items* (not messages), each
  typed `message`, `function_call`, or `function_call_output`;
- an **input audio buffer** — currently-uncommitted user audio;
- a **response generator** — that can be told to run, or that runs itself.

The client drives this with a handful of events:

- `session.update` — the closest thing to a "system prompt." Sets `instructions`,
  `voice`, `modalities` (`["text","audio"]` or `["text"]`), `input_audio_format`
  (e.g. `pcm16`, `g711_ulaw`), `output_audio_format`, `input_audio_transcription`,
  `turn_detection`, `tools`, `tool_choice`, `temperature`,
  `max_response_output_tokens`. Can be sent at any time, not just at the start.
- `input_audio_buffer.append` — append a base64-encoded chunk of audio. Sent
  continuously, many small chunks per second.
- `input_audio_buffer.commit` — finalize the buffer into a user item.
- `input_audio_buffer.clear` — discard the pending buffer.
- `conversation.item.create` — splice an arbitrary item into the conversation: a
  text user message, an imported assistant turn, or a `function_call_output`. This
  is also how you do **text-modality input** in a voice session.
- `response.create` — ask the model to generate now. Optional fields override
  session defaults for this one response.
- `response.cancel` — interrupt the in-flight response.

A single user turn produces **dozens of server events** back. The important ones:

| Event | Meaning |
|---|---|
| `session.created` / `session.updated` | handshake done / your `session.update` applied |
| `input_audio_buffer.speech_started` / `speech_stopped` / `committed` | server VAD detected speech edges, auto-committed |
| `conversation.item.input_audio_transcription.completed` | Whisper transcript of what the user said |
| `response.created` | a response started generating |
| `response.audio.delta` | base64 PCM audio chunk of the model's voice (streaming) |
| `response.audio_transcript.delta` / `.done` | the text of what the model is saying (for captions) |
| `response.text.delta` / `.done` | text output (text modality) |
| `response.function_call_arguments.delta` / `.done` | the model is calling a tool |
| `response.done` | response finished; includes token usage |
| `rate_limits.updated` | unsolicited rate-limit signal |
| `error` | structured error, mid-session |

```python
# Python — OpenAI Realtime (WebSocket)
from openai import AsyncOpenAI

client = AsyncOpenAI()

async with client.beta.realtime.connect(model="gpt-4o-realtime-preview") as conn:
    # Configure the session — instructions, voice, VAD, tools.
    await conn.session.update(session={
        "modalities": ["text", "audio"],
        "instructions": "You are a friendly voice assistant. Keep replies short.",
        "voice": "alloy",
        "input_audio_format": "pcm16",
        "output_audio_format": "pcm16",
        "input_audio_transcription": {"model": "whisper-1"},
        "turn_detection": {
            "type": "server_vad",       # server decides when the user stopped
            "threshold": 0.5,
            "prefix_padding_ms": 300,
            "silence_duration_ms": 500,
            "create_response": True,    # ... and auto-replies
        },
    })

    # Stream microphone audio up continuously. With server VAD we never
    # call commit() or response.create() — the server does both.
    async def pump_mic():
        async for chunk in microphone_pcm16():          # your capture loop
            await conn.input_audio_buffer.append(audio=base64.b64encode(chunk).decode())

    asyncio.create_task(pump_mic())

    # The receive loop runs for the whole session, many responses long.
    async for event in conn:
        if event.type == "response.audio.delta":
            speaker.play(base64.b64decode(event.delta))           # hear the model
        elif event.type == "response.audio_transcript.delta":
            ui.append_caption(event.delta)                        # show captions
        elif event.type == "conversation.item.input_audio_transcription.completed":
            ui.append_user_line(event.transcript)
        elif event.type == "response.done":
            pass                                                  # one of many
```

```ts
// TS — OpenAI Realtime (WebSocket)
import OpenAI from "openai";
const client = new OpenAI();

const conn = await client.beta.realtime.connect({ model: "gpt-4o-realtime-preview" });

conn.send({
  type: "session.update",
  session: {
    modalities: ["text", "audio"],
    instructions: "You are a friendly voice assistant. Keep replies short.",
    voice: "alloy",
    input_audio_format: "pcm16",
    output_audio_format: "pcm16",
    turn_detection: {
      type: "server_vad",
      threshold: 0.5,
      prefix_padding_ms: 300,
      silence_duration_ms: 500,
      create_response: true,
    },
  },
});

// Bidirectional: register handlers AND push audio whenever it's ready.
conn.on("response.audio.delta", (e) => speaker.play(Buffer.from(e.delta, "base64")));
conn.on("response.audio_transcript.delta", (e) => ui.appendCaption(e.delta));
conn.on("error", (e) => console.error(e.error));

micStream.on("data", (chunk: Buffer) =>
  conn.send({ type: "input_audio_buffer.append", audio: chunk.toString("base64") }),
);
```

Google's **Gemini Live API** has the same shape with different names: open a session
with `client.aio.live.connect(...)`, send realtime input, iterate received events.

```python
# Python — Gemini Live
from google import genai

client = genai.Client()
config = {"response_modalities": ["AUDIO"], "system_instruction": "Be brief."}

async with client.aio.live.connect(model="gemini-2.0-flash-live-001",
                                    config=config) as session:
    async def pump_mic():
        async for chunk in microphone_pcm16():
            await session.send_realtime_input(
                audio={"data": chunk, "mime_type": "audio/pcm;rate=16000"})
    asyncio.create_task(pump_mic())

    async for response in session.receive():
        if response.data:                       # raw output audio bytes
            speaker.play(response.data)
        if response.server_content and response.server_content.turn_complete:
            pass                                # turn boundary
```

The two **turn-detection modes** are the central design choice:

- **Server VAD** (`turn_detection: { type: "server_vad", ... }`) — the server runs
  voice-activity detection, auto-commits the buffer when the user goes silent for
  `silence_duration_ms`, and (if `create_response: true`) auto-fires `response.create`.
  The client just streams audio forever and listens. **The server initiates
  responses.** This is what makes it feel like a phone call.
- **Manual / push-to-talk** (`turn_detection: null` or `{ "type": "none" }`) — no
  server VAD. The client decides when the user is done, calls
  `input_audio_buffer.commit`, then `response.create`. Used when the UI has an
  explicit "hold to talk" button.

### What varies across providers

- **Who initiates.** Server-VAD mode means the server creates responses with no
  client request at all. Manual mode puts the client back in control. Most other
  capabilities in this doc set assume the *client* triggers every generation; here
  that assumption breaks.
- **Audio formats.** OpenAI: `pcm16`, `g711_ulaw`, `g711_alaw`. Gemini: PCM at
  specific sample rates declared via mime type. Telephony integrations (Twilio,
  SIP) push G.711 µ-law; browsers push PCM.
- **Voice catalogs** differ entirely (`alloy`/`echo`/`shimmer`/… vs Gemini's set)
  and are not interchangeable.
- **Items vs messages.** OpenAI Realtime models the conversation as typed *items*;
  Gemini Live uses turn-based `server_content`. Neither matches the role/content
  message array of Chat Completions.
- **Transcription.** OpenAI optionally runs Whisper on input audio and emits a
  separate transcript event; the model's *own* speech is transcribed via
  `response.audio_transcript.*`. These are two different transcript streams.

### What's hard

- **Concurrency.** You must pump microphone audio up **and** drain server events
  down at the same time, indefinitely. A naive `for event in conn` loop that also
  tries to send will deadlock; you need two concurrent tasks over one connection.
- **The connection *is* the session.** There is no session id to reconnect to. If
  the WebSocket drops, the server-side conversation is gone (see the frontier
  section below).
- **Audio is not a "result."** A complete-artifact media type (a whole audio file)
  cannot represent a stream of PCM chunks arriving 50ms apart. Streaming audio has
  to be handled as a side channel, separate from any final "answer" object.
- **Clock alignment.** Captions (`audio_transcript.delta`) and audio
  (`audio.delta`) arrive as separate event streams that must be re-synced for the
  UI, and barge-in (next section) requires knowing *how much* of the audio the user
  actually heard.

---

## ◆ Barge-in, interruption & mid-session mutation

**Goal:** "The user starts talking over the model. I want to cut the model off
instantly, and I want the model's *memory* to reflect only what the user actually
heard — not the full sentence it was about to finish."

### How it's done today

Three distinct moves, all sent over the *same* open connection while a response is
in flight:

1. **`response.cancel`** — stop generating the current response now. The server
   stops emitting audio/text and emits `response.done` with `status: "cancelled"`.
   The session continues; a new response can follow.
2. **`conversation.item.truncate`** — the subtle one. The server has already
   *streamed* (say) 4 seconds of audio, but the user's speaker only *played* 1.4
   seconds before the user interrupted. You tell the server "the assistant item was
   truncated at 1400ms" so the conversation memory reflects what the user heard, not
   what was generated. Without this, the model thinks it said things the user never
   heard.
3. **Live `session.update`** — change `voice`, `instructions`, `temperature`,
   `tools`, or VAD sensitivity mid-conversation, with no reconnect.

```python
# Python — barge-in: cancel + truncate to the played position
# Driven by the server's own VAD telling us the user started talking.
async for event in conn:
    if event.type == "input_audio_buffer.speech_started":
        if current_response_active:
            await conn.response.cancel()                 # stop the model now
            await conn.conversation.item.truncate(
                item_id=current_assistant_item_id,
                content_index=0,
                audio_end_ms=ms_actually_played,          # what the user heard
            )
    elif event.type == "response.audio.delta":
        ms_actually_played = speaker.play(base64.b64decode(event.delta))

# Mid-session mutation — switch voice on the fly, no reconnect:
await conn.session.update(session={"voice": "shimmer"})
```

```ts
// TS — barge-in
conn.on("input_audio_buffer.speech_started", () => {
  if (currentResponseActive) {
    conn.send({ type: "response.cancel" });
    conn.send({
      type: "conversation.item.truncate",
      item_id: currentAssistantItemId,
      content_index: 0,
      audio_end_ms: msActuallyPlayed,
    });
  }
});

// Change instructions partway through the call:
conn.send({ type: "session.update", session: { instructions: "Switch to Spanish." } });
```

### What varies across providers

- **Truncation granularity.** OpenAI truncates by *milliseconds of audio played*.
  Gemini Live signals interruption differently (the server emits an interruption
  flag and discards queued audio on its side).
- **Who detects the barge-in.** With server VAD, the server emits
  `speech_started` and the client reacts; with WebRTC the user-agent's echo
  cancellation matters so the model's own voice doesn't trigger a false barge-in.
- **What `cancel` is scoped to.** Cancelling *one response* is different from
  closing the *whole session*. Most SDKs surface both but with very different APIs.

### What's hard

- **The played-position problem.** The client must track how many milliseconds of
  audio it actually pushed to the speaker (accounting for the playback buffer) to
  truncate correctly. This is bookkeeping no HTTP API ever needed.
- **Races.** Between deciding to cancel and the cancel landing, more `audio.delta`
  events may already be in flight. The client has to discard audio that arrives
  *after* it decided to interrupt.
- **Cancel vs close.** "Stop this response" and "end the conversation" are two
  operations with completely different consequences, and conflating them ends the
  call by accident.

---

## ◆ Tools in a realtime session

**Goal:** "The voice model needs to call a function — look up the weather, hit my
calendar — and keep talking naturally while I run it."

### How it's done today

Tools are declared in `session.update` (or per-response in `response.create`), the
same JSON-Schema function shape as everywhere else. But the *delivery* and the
*loop* are different. The tool call arrives as **events**, and — critically — the
response is **not paused** while you run the tool:

1. During generation the server emits `response.output_item.added` with
   `item.type == "function_call"`.
2. The server streams `response.function_call_arguments.delta` events.
3. The server emits `response.function_call_arguments.done` with the final
   `arguments` string, a `call_id`, and the function `name`.
4. The client runs the tool **on its own time** — the model may still be emitting
   audio.
5. The client sends `conversation.item.create` with an item of type
   `function_call_output` carrying the `call_id` and `output`.
6. The client sends `response.create` to ask the model to continue with the result.

```python
# Python — realtime tool loop
async for event in conn:
    if event.type == "response.function_call_arguments.done":
        args = json.loads(event.arguments)
        result = await run_tool(event.name, args)        # your dispatch
        await conn.conversation.item.create(item={
            "type": "function_call_output",
            "call_id": event.call_id,
            "output": json.dumps(result),
        })
        await conn.response.create()                       # let the model react
```

```ts
// TS — realtime tool loop
conn.on("response.function_call_arguments.done", async (e) => {
  const args = JSON.parse(e.arguments);
  const result = await runTool(e.name, args);
  conn.send({
    type: "conversation.item.create",
    item: { type: "function_call_output", call_id: e.call_id, output: JSON.stringify(result) },
  });
  conn.send({ type: "response.create" });
});
```

**Contrast with the synchronous Chat tool loop** (`02-tools-and-agents.md`): there,
the model's turn *ends* with a `tool_calls` stop reason; the client runs the tools;
the client makes a *new request* with the tool results appended; the model's *next*
turn begins. The loop is strictly turn-by-turn, request-by-request. In realtime,
the tool call is one event among a continuous stream, the model is not blocked
waiting, there is no new HTTP request, and the result is *spliced into a live
server-side conversation* via `conversation.item.create`. The tool result is an
**outbound event with a correlation id**, not a return value occupying a slot in a
request body.

### What varies across providers

- **Pause semantics.** OpenAI does not pause generation around a function call; the
  client controls when the result goes back. Some other realtime stacks effectively
  block.
- **Parallel calls.** Chat Completions can emit several `tool_calls` in one turn;
  realtime tends to issue one `function_call` per output item.
- **Where results land.** Realtime: a `function_call_output` *item* in the
  server-side conversation. Chat/Responses: a `tool` message / `function_call_output`
  in the *next request body*.

### What's hard

- **No synchronous return slot.** The mental model "call function, get value back
  into the response" does not exist. You emit an event and *separately* ask for
  continuation; correlation is by `call_id` only.
- **Ordering with audio.** A tool result injected while the model is still speaking
  the previous response can interleave oddly; you usually wait for `response.done`
  before `response.create` for the follow-up.
- **Latency.** A slow tool stalls the conversation audibly. There is no spinner —
  there is dead air on a phone call.

---

## ◆ Cascaded voice pipelines (STT → LLM → TTS)

**Goal:** "I want a voice agent assembled from separate speech-to-text, LLM, and
text-to-speech stages, so I can swap any stage's provider and run over a phone line."

### How it's done today

The realtime section above used a **single speech-to-speech model** — one model takes
audio in and emits audio out. The other mainstream way to build a voice agent inverts
that: keep three independent stages and wire them together yourself. **Pipecat** is the
canonical stack for this. A voice agent is a **pipeline of frame processors**, and
typed **frames** (audio, transcription, text, LLM-context, control) flow through it in
order:

```
transport_in → VAD → STT → LLM (context aggregator) → TTS → transport_out
```

Each stage is a processor that consumes some frame types and emits others: the STT
service turns `AudioRawFrame`s into `TranscriptionFrame`s, the LLM service turns the
aggregated context into streamed `TextFrame`s, the TTS service turns text back into
audio frames. **Interruption / turn-taking is built in** — VAD-driven barge-in cancels
in-flight TTS and flushes the downstream queue without you writing the cancel/truncate
bookkeeping the STS path needs by hand (the realtime section above). Pipecat ships
**transports** for WebRTC (Daily), raw WebSocket, and **telephony** (Twilio / phone
over G.711), plus client SDKs (JS/React, iOS, Android). Notably, Pipecat can **also
drive a speech-to-speech model in the same pipeline** — the STS model is just another
processor that replaces the STT+LLM+TTS trio — so the pipeline shape is the same whether
the middle is cascaded or a single STS model.

```python
# Python — Pipecat cascaded voice pipeline
from pipecat.pipeline.pipeline import Pipeline
from pipecat.pipeline.task import PipelineTask, PipelineParams
from pipecat.pipeline.runner import PipelineRunner
from pipecat.services.deepgram.stt import DeepgramSTTService
from pipecat.services.openai.llm import OpenAILLMService
from pipecat.services.elevenlabs.tts import ElevenLabsTTSService
from pipecat.transports.services.daily import DailyTransport, DailyParams
from pipecat.audio.vad.silero import SileroVADAnalyzer

transport = DailyTransport(
    room_url, token, "voice-bot",
    DailyParams(audio_in_enabled=True, audio_out_enabled=True,
                vad_analyzer=SileroVADAnalyzer()),   # turn detection
)

stt = DeepgramSTTService(api_key=DEEPGRAM_KEY)              # swap any STT here
llm = OpenAILLMService(api_key=OPENAI_KEY, model="gpt-4o") # swap any LLM here
tts = ElevenLabsTTSService(api_key=ELEVEN_KEY, voice_id="...")  # swap any TTS here

context = OpenAILLMContext([{"role": "system", "content": "Be brief."}])
aggregator = llm.create_context_aggregator(context)

pipeline = Pipeline([
    transport.input(),          # mic frames in
    stt,                        # audio  → transcription frames
    aggregator.user(),          # transcription → LLM context
    llm,                        # context → streamed text frames
    tts,                        # text → audio frames
    transport.output(),         # audio out to the user
    aggregator.assistant(),     # assistant reply → back into context
])

task = PipelineTask(pipeline, PipelineParams(allow_interruptions=True))  # barge-in
await PipelineRunner().run(task)
```

For multi-step conversations, **`pipecat-flows`** layers a **state-machine** on top:
named nodes, per-node instructions and tools, and edges/transitions, so a structured
flow (collect name → verify account → route) is declared rather than coded ad hoc
inside the LLM stage.

### What varies across providers

- **Cascaded vs single STS — the architecture fork.** Cascaded means you **compose and
  swap each stage independently** (any STT × any LLM × any TTS) and get fine control of
  each — at the cost of more moving parts and **higher round-trip latency** (three
  network hops + buffering, not one). A single STS model (the realtime section above)
  gives **lower latency and tighter prosody** but **less control and far fewer
  providers**. This is the central decision when building a voice agent.
- **Transports.** WebRTC (Daily) for browser/mobile with hardware-grade audio,
  raw WebSocket for server-to-server, telephony (Twilio/SIP, G.711 µ-law) for phone —
  the same pipeline runs over any of them by swapping the transport processor.
- **Stage providers.** STT (Deepgram, Whisper, Google, AssemblyAI), LLM (any chat
  model), and TTS (ElevenLabs, Cartesia, PlayHT, Azure) each have their own SDKs,
  voices, and streaming behavior.

### What's hard

- **The latency budget spans three hops.** End-to-end response time is STT finalization
  + LLM TTFT + TTS first-audio, plus the queue between each — and it all has to land
  under the conversational <300ms-feel budget. The STS model collapses this to one hop.
- **Interruption, echo, and turn detection are yours to wire.** VAD has to fire,
  in-flight TTS has to be cancelled and its queued audio flushed, and the model's own
  voice must not trigger a false barge-in — the framework provides the hooks, but the
  budget and tuning are on you.
- **Aligning transcripts with audio.** What the user said (STT) and what the bot said
  (the TTS'd text) must be stitched back into one ordered context for the LLM, across
  stages that emit on different clocks.
- **Provider-stage glue.** Every STT/LLM/TTS combination is a different set of SDKs,
  auth, audio formats, and streaming idioms to normalize into uniform frames.

---

## ◆ Unified voice abstractions

**Goal:** "I want one voice interface and the freedom to mix and swap TTS / STT / STS
providers."

### How it's done today

Where Pipecat composes voice as a *pipeline*, another approach is a single **voice
object** with a fixed verb surface, backed by pluggable providers. **Mastra's voice
layer** is representative: `speak()` for TTS, `listen()` for STT, and `connect()` /
`send()` / `on('speaker', …)` for realtime STS — the same interface regardless of which
provider is behind it.

```ts
// TS — TTS: text → audio stream
import { OpenAIVoice } from "@mastra/voice-openai";

const voice = new OpenAIVoice({
  speechModel: { name: "tts-1-hd", apiKey: process.env.OPENAI_API_KEY },
  speaker: "alloy",
});
const audio = await voice.speak("Hello there.", {
  speaker: "nova",
  properties: { speed: 1.1, pitch: "high" },
});   // returns a ReadableStream of audio
```

```ts
// TS — STT: audio stream → transcript
const transcript = await voice.listen(createReadStream("./question.mp3"), {
  filetype: "mp3",
});
```

Attaching a voice to an agent makes the agent speakable/listenable:

```ts
// TS — voice as an agent capability
const agent = new Agent({
  name: "support",
  instructions: "You are a friendly voice assistant.",
  model: openai("gpt-4o"),
  voice: new OpenAIVoice(),
});
```

Realtime STS providers (e.g. OpenAI Realtime, AWS Nova Sonic) use the streaming verbs —
the same object, a different method set:

```ts
// TS — realtime STS over the same interface
import { OpenAIRealtimeVoice } from "@mastra/voice-openai-realtime";

const voice = new OpenAIRealtimeVoice();
await voice.connect();                                  // open the live session
voice.on("speaker", ({ audio }) => speaker.play(audio)); // model's voice streams back
voice.on("writing", ({ text }) => ui.appendCaption(text));
await voice.send(getMicrophoneStream());                // stream mic up continuously
```

**`CompositeVoice`** is the mix-and-match piece: take STT from one provider and TTS
from another behind one object — or plug in AI SDK models directly.

```ts
// TS — mix providers: Deepgram STT in, ElevenLabs TTS out
import { CompositeVoice } from "@mastra/core/voice";
import { DeepgramVoice } from "@mastra/voice-deepgram";
import { ElevenLabsVoice } from "@mastra/voice-elevenlabs";

const voice = new CompositeVoice({
  input: new DeepgramVoice(),     // STT provider
  output: new ElevenLabsVoice(),  // TTS provider
});

// ...or compose AI SDK models:
import { openai } from "@ai-sdk/openai";
import { elevenlabs } from "@ai-sdk/elevenlabs";
const voice2 = new CompositeVoice({
  input: openai.transcription("whisper-1"),
  output: elevenlabs.speech("eleven_turbo_v2"),
});
```

There are 10–15+ providers behind this one surface — OpenAI, ElevenLabs, Deepgram,
Google, Azure, PlayAI, Cloudflare, AWS Nova Sonic (STS), and more — each wired to the
same `speak` / `listen` / `connect` verbs.

### What varies across providers

- **Capability coverage.** Not every provider does every verb. Roughly:

  | Provider | TTS (`speak`) | STT (`listen`) | Realtime STS |
  |---|---|---|---|
  | OpenAI | ✓ | ✓ | ✓ (Realtime) |
  | ElevenLabs | ✓ | — | — |
  | Deepgram | ✓ | ✓ | — |
  | Google / Azure | ✓ | ✓ | — |
  | PlayAI / Speechify / Murf | ✓ | — | — |
  | AWS Nova Sonic | ✓ | ✓ | ✓ |

- **Speaker / voice naming.** Voice catalogs are provider-specific and not
  interchangeable (`alloy`/`nova` vs ElevenLabs voice IDs vs Azure neural names).
- **Response formats and streaming.** Some return a `ReadableStream`, some a buffer;
  output formats (PCM/MP3/WAV/Opus) and whether streaming is supported differ per
  provider — as do per-call options like `speed`, `pitch`, and `responseFormat`.

### What's hard

- **Normalizing across providers.** Speakers, audio formats, and streaming behavior all
  have to be flattened to one `speak`/`listen`/`connect` shape without losing what makes
  each provider useful.
- **Not every provider does realtime.** The streaming verbs (`connect`/`send`/`on`)
  only exist for STS-capable providers; a TTS-only provider behind the same interface
  simply can't fulfill them, so the uniform surface has holes.
- **The abstraction leaks.** Provider-specific options (voice IDs, model names,
  format flags, prosody controls) surface through generic `properties` / config bags,
  so writing genuinely provider-portable code is harder than the unified API suggests.

---

## ▲ Why request/response cannot model this

**Goal:** understand *why* a `(request) -> response` or even `(request) ->
Stream<chunk>` signature is structurally unable to express a live conversation —
not merely awkward, but wrong-shaped.

### The five structural reasons

1. **Concurrent duplex.** The user can stream audio *in* while the model streams
   audio *out*. There is no point where "the request finishes" and "the response
   begins." Both directions are open at once. A function signature has one input and
   one output; this has two simultaneous, ongoing streams.
2. **Server-initiated events with no triggering request.** `rate_limits.updated`,
   `input_audio_buffer.speech_started`, a mid-session `error`, a side-channel
   `session.updated` — these are unsolicited pushes. There is no request they are the
   response *to*. A request/response shape has nowhere to put them.
3. **Many responses per session.** A single connection produces N user turns, each
   yielding 0, 1, or several responses (one cancelled, one retried, one completed).
   `(request) -> response` is 1:1 by construction; this is 1:N, and N is not known
   in advance.
4. **Server-authoritative state.** The conversation lives on the server as an
   ordered item list. The client mutates it (`conversation.item.create` /
   `.truncate` / `.delete`) but never holds the canonical copy. A request/response
   interface implicitly assumes the request *carries* the state it needs.
5. **Connection loss = state loss.** There is no session id to resume. Drop the
   socket and the conversation is gone. A stateless `(request) -> response` call has
   nothing to lose on disconnect; a live session has everything to lose.

The conclusion the SDKs themselves reached: `client.chat.completions.create(...)`
returns a single object or a chunk iterator; `client.beta.realtime.connect(...)`
returns an **open connection with bidirectional send/receive**. That difference is
not stylistic — the protocol does not admit a request/response shape, so the SDK
surface cannot pretend it does. An event-stream shape can express single-shot as a
degenerate case (`send(one request event); collect until done`); the inverse cannot
express realtime.

### What varies

- The *cardinality* of unsolicited events: a text SSE stream has a few
  (`error` mid-stream); realtime has ~15–25 event types, many server-initiated.
- Whether state is recoverable: Realtime loses it on disconnect; Responses-with-id
  (`03-state-sessions-memory.md`) keeps it server-stored and addressable.

### What's hard

- **Exhaustiveness.** With 20+ event types, stringly-typed dispatch silently drops
  the ones you forgot to handle. A tagged-union receive surface is what keeps this
  honest, but few stacks provide it.
- **Backpressure.** If the client can't drain events as fast as the server pushes
  (slow tool, slow audio sink), the connection buffers and latency climbs — a
  concern that simply doesn't exist for a one-shot POST.

---

## ★ Transport taxonomy

**Goal:** know, for any given use case, *which wire shape* it needs — and why the
choice is forced rather than free.

### How it's done today

Five transports are in production use. They differ along four axes: **latency
budget**, **direction**, **server state**, and **when the transport becomes
mandatory** (vs merely an option).

| Transport | Latency budget | Direction | Server state | When it's mandatory |
|---|---|---|---|---|
| **HTTP single-shot** <br>(Chat Completions / Anthropic Messages, non-streaming) | Whole-response: seconds to minutes | Request → response, then close | None (client owns history) | Never *mandatory* — it's the floor. Used when TTFT doesn't matter: batch, extraction, pipelines. |
| **HTTP + SSE** <br>(Chat Completions stream, Responses stream, Anthropic Messages stream) | TTFT ~100–500ms; per-token ~10–50ms | Request → server-push token stream, then close | Optional; addressable-by-id for Responses; otherwise none | When the user must *see tokens as they arrive* but never needs to interject mid-stream. |
| **HTTP + chunked JSON** <br>(Gemini `streamGenerateContent`) | Same as SSE | Request → server-push chunk stream, then close | None | Same niche as SSE. Differs only in framing: a streamed **JSON array** of `GenerateContentResponse` chunks rather than `text/event-stream` framing. |
| **WebSocket (Realtime)** <br>(OpenAI Realtime; Gemini Live; agent control planes) | Per-event ~20–100ms; bidirectional concurrency | Full duplex — client and server push independently | Server-side session is authoritative | When you need mid-stream client→server messages: barge-in, live tool injection, side-channel commands, server-push notifications. |
| **WebSocket (Responses mode)** <br>(OpenAI Responses over `wss://…/v1/responses`) | Same TTFT/per-token profile as SSE, but **warm socket** removes per-call setup on continuations | Ordinary Responses request → server-push event stream; next turn re-uses the same socket. **One in-flight response per connection.** | Connection-local cache keeps the most recent response warm (works with ZDR, in-memory only) | Never *mandatory* — it's a latency optimization for long, tool-heavy agent rollouts (20+ tool calls). Not bidirectional audio. See below. |
| **WebRTC** <br>(OpenAI Realtime audio; LiveKit-mediated agents) | Media RTT ~50–150ms; data channel ~ WS | Full duplex, **media and data on separate planes** | Same as Realtime WebSocket | Browser/mobile voice that needs hardware-accelerated capture, jitter buffering, packet-loss concealment, echo cancellation. |

The bottom two rows are where the model stops being "a thing you call."

### The WebRTC media-vs-data split

WebRTC is the one transport that **splits the connection in two**:

- An `RTCDataChannel` (OpenAI names it `"oai-events"`) carries the *same JSON event
  grammar* as the WebSocket transport — `session.update`, `response.create`, tool
  events, etc.
- An `RTCPeerConnection` audio **track** carries the actual media as RTP. The audio
  bytes do **not** flow through the JSON channel. (Correspondingly, OpenAI does
  *not* emit `response.audio.delta` over WebRTC — the audio is on the media track,
  not the event stream.)

This separation is the whole point: the control plane (events) and the media plane
(audio) have different reliability and latency needs, and the browser's media stack
(echo cancellation, jitter buffer, packet-loss concealment) operates on the RTP
track for free. Setup is also different: you mint an **ephemeral key** server-side,
then exchange an SDP offer/answer, rather than authenticating a header on a socket
upgrade.

```python
# Python — WebSocket handshake (server-to-server): one connection, JSON both ways
# wss://api.openai.com/v1/realtime?model=gpt-4o-realtime-preview
#   Authorization: Bearer <api-key>
#   OpenAI-Beta: realtime=v1
# After the upgrade, every frame is a JSON event with a "type" field.
```

```ts
// TS — WebRTC handshake (browser): ephemeral key, then SDP, then two planes
const ek = await fetch("/api/ephemeral-key").then(r => r.json());   // minted server-side
const pc = new RTCPeerConnection();

pc.ontrack = (e) => (audioEl.srcObject = e.streams[0]);             // MEDIA plane (audio)
pc.addTrack(micStream.getAudioTracks()[0]);                         // mic up, on the track

const events = pc.createDataChannel("oai-events");                  // DATA plane (JSON events)
events.onmessage = (e) => handleEvent(JSON.parse(e.data));
events.onopen = () => events.send(JSON.stringify({
  type: "session.update",
  session: { modalities: ["audio", "text"], voice: "alloy" },
}));

const offer = await pc.createOffer();
await pc.setLocalDescription(offer);
const answer = await fetch("https://api.openai.com/v1/realtime", {
  method: "POST",
  body: offer.sdp,
  headers: { Authorization: `Bearer ${ek.client_secret.value}`, "Content-Type": "application/sdp" },
}).then(r => r.text());
await pc.setRemoteDescription({ type: "answer", sdp: answer });
```

### Responses over WebSocket

This is a **different beast from the Realtime WebSocket above**, and the shared
transport (a `wss://` socket carrying JSON events) hides how different. The Realtime
WS is bidirectional audio with its own event grammar (`input_audio_buffer.append`,
`response.audio.delta`, server VAD, barge-in). The **Responses WebSocket** carries
the *ordinary Responses request/response semantics* — same body, same tools, same
`previous_response_id` chaining — just over a **persistent, warm socket** instead of
a fresh HTTP request per turn. There is no audio, no VAD, no duplex: it is plain
Responses, made faster.

The mechanics: open a socket to `wss://api.openai.com/v1/responses`. The first
message is a normal Responses body sent as a `response.create` event. The server
streams the same Responses event sequence back (`response.created`,
`response.output_text.delta`, … `response.completed`). When the model calls a tool,
you run it and send **another `response.create` on the same socket**, carrying
`previous_response_id` plus the new `function_call_output` item — instead of opening
a fresh HTTP request. A **connection-local cache** keeps the most recent response
warm, so each continuation does less setup; for rollouts with 20+ tool calls this is
reported ~40% faster end-to-end than re-POSTing each turn.

```python
# Python — Responses over a persistent WebSocket (latency mode, NOT audio)
import json, websockets

async with websockets.connect(
    "wss://api.openai.com/v1/responses",
    additional_headers={"Authorization": f"Bearer {API_KEY}"},
) as ws:
    # First turn: a normal Responses body, sent as a response.create event.
    await ws.send(json.dumps({
        "type": "response.create",
        "response": {
            "model": "gpt-5",
            "input": [{"role": "user", "content": "Audit this repo for dead code."}],
            "tools": tools,          # same JSON-Schema tool defs as plain HTTP
            "stream": True,
        },
    }))

    async for raw in ws:
        event = json.loads(raw)
        if event["type"] == "response.output_text.delta":
            ui.append(event["delta"])
        elif event["type"] == "response.completed":
            break                    # first event of the turn; loop continues for tools

    # Tool turn: continue on the SAME socket — warm cache, no new HTTP request.
    await ws.send(json.dumps({
        "type": "response.create",
        "response": {
            "model": "gpt-5",
            "previous_response_id": last_response_id,
            "input": [{"type": "function_call_output",
                       "call_id": call_id, "output": result}],
            "stream": True,
        },
    }))
```

```ts
// TS — Responses over a persistent WebSocket
import WebSocket from "ws";

const ws = new WebSocket("wss://api.openai.com/v1/responses", {
  headers: { Authorization: `Bearer ${process.env.OPENAI_API_KEY}` },
});

ws.on("open", () => {
  // First message is just a Responses body wrapped as response.create.
  ws.send(JSON.stringify({
    type: "response.create",
    response: {
      model: "gpt-5",
      input: [{ role: "user", content: "Audit this repo for dead code." }],
      tools,                 // identical to the plain-HTTP Responses call
      stream: true,
    },
  }));
});

ws.on("message", (raw) => {
  const event = JSON.parse(raw.toString());
  if (event.type === "response.output_text.delta") ui.append(event.delta);
  // On a tool call, send another response.create on THIS socket with
  // previous_response_id + the function_call_output item.
});
```

**Constraints:** exactly **one in-flight response per connection** — parallel work
needs multiple sockets; a **~60-minute connection cap**; works with
Zero-Data-Retention because the warm cache is **in-memory only** (contrast the
background-job path below, which requires `store=True`). If the workflow is
one-request-one-answer, plain HTTP is fine and the socket buys nothing; the WebSocket
mode pays off only for long, tool-heavy agent runs where per-turn setup dominates.

**Don't conflate the two WebSockets.** Realtime WS = bidirectional audio + its own
event grammar + server-initiated turns (the sections above). Responses WS = the same
Responses semantics you'd send over HTTP, on a warm socket for latency. Same wire
technology, opposite purpose.

### What varies across providers

- **Streaming framing.** OpenAI / Anthropic stream over **SSE** (`text/event-stream`,
  `data:` lines). Gemini streams a **chunked JSON array** from
  `streamGenerateContent`. Same latency profile, different parser.
- **Event richness.** Chat Completions emits opaque `chat.completion.chunk` delta
  diffs to reassemble (~handful of shapes). Responses emits ~25–35 *semantically
  typed* SSE events. Anthropic Messages stream has ~10. Realtime WS has ~15–25.
  Cardinality is similar between Responses SSE and Realtime WS; the difference is
  **direction** — Responses SSE is one-directional (server→client) and ends when the
  response is done.
- **WebRTC availability.** OpenAI offers a first-party WebRTC path; most other
  realtime stacks route media through a third party (LiveKit) instead.

### What's hard

- **Transport is a property of the *provider*, not the call.** Whether `Foo(...)`
  goes over HTTP-SSE or a pre-opened WebSocket is a deployment/provider concern, but
  the event vocabulary the application code handles should ideally be the same
  across transports (this is exactly what lets OpenAI share one event grammar across
  WS and WebRTC).
- **State location is an *orthogonal* axis** (see below) — you can't read it off the
  transport.
- **WebRTC is mostly a browser/mobile concern.** A server-side runtime almost always
  uses WebSocket; WebRTC matters only when emitting client code that runs in a
  user-agent with a real microphone and speaker.

---

## The bidirectionality fault line

**Goal:** locate the single discrete jump — from "SSE-land" to "WebSocket-land" —
and know exactly what shoves a use case across it.

### It is a cliff, not a slope

SSE *feels* almost bidirectional: request bodies can be huge, responses stream
token-by-token, and for interactive chat it is the industry sweet spot. But there
is one thing SSE structurally cannot do: **send a message from client to server
mid-stream.** Once the request is sent, the client's mouth is taped shut until the
response completes. The only way to "say something" mid-generation over HTTP is to
*open a second request* — another full round-trip — which is too slow for a
conversation.

Four use cases force the crossing. If a use case needs **any one** of these, no
amount of careful HTTP design closes the gap — you are in WebSocket/WebRTC-land:

1. **Barge-in** — the user interrupts the model mid-speech, and you must cancel +
   truncate *now*, not after the model finishes.
2. **Mid-stream tool-result injection** — pushing a `function_call_output` into a
   live conversation while the model is still generating.
3. **Side-channel commands** — `response.cancel`, `session.update` to change voice
   or temperature mid-response, adjusting VAD sensitivity on the fly.
4. **Unsolicited server events** — `rate_limits.updated`, `speech_started`, a
   mid-session `error`: pushes with no triggering request.

### The three latency regimes

The transports cluster into three budgets, and the budget — more than anything
else — dictates the transport:

| Regime | Budget | Transport that suffices |
|---|---|---|
| **Patience** | Seconds are fine | HTTP single-shot. Batch jobs, extraction, offline pipelines. |
| **Reading** | ~200ms TTFT, then tokens flowing | HTTP + SSE / chunked JSON. The dominant interactive-chat shape. WebSocket buys nothing here. |
| **Conversational** | <300ms turn-take | WebSocket is **mandatory** (SSE is unidirectional, so barge-in costs a whole new request = another RTT). For audio specifically, **WebRTC** is preferred because the media path is independent of the control path. |

### Server state is a *separate* axis

A common confusion: people assume "WebSocket ⇒ server state" and "HTTP ⇒
stateless." Not so. Transport (how bytes flow) and state location (who owns the
conversation) are **orthogonal**:

| | No server state | Server state |
|---|---|---|
| **HTTP single-shot** | Chat Completions, Anthropic Messages | Responses + `previous_response_id` |
| **WebSocket** | (hypothetical — just HTTP-streaming over WS) | OpenAI Realtime, Gemini Live |

State location changes *how the client formulates each turn* (thread full history
vs pass a handle vs rely on a live session); transport changes *how data moves on
the wire*. A complete picture of any provider needs both coordinates. The three
state-location buckets themselves — client-owned, server-stored-by-id,
server-session — are covered in `03-state-sessions-memory.md`.

---

## ◆ Async jobs: background + poll

**Goal:** "My request may take minutes (a big analysis, a long tool run). I don't
want to hold a connection open; I want to fire it, get an id, and poll for the
result — and survive a client disconnect."

This is a **fourth request lifecycle**, distinct from the three above. Sync is
one request, one blocking response. Streaming is one request, a server-push token
stream over one held connection. Bidirectional (Realtime) is a live duplex session.
**Background + poll** is none of these: the create call returns *immediately* with a
job id, the work runs server-side with **no connection held open**, and the client
comes back later to read the result. It is the durability/resumability story for a
single long call — if the client process dies, the job keeps running and can be
retrieved by id afterward.

### How it's done today

OpenAI Responses exposes this as `background=True`. The create call returns at once
with an id and a `status` of `queued`; you then poll `responses.retrieve(id)` until
`status` leaves `queued` / `in_progress` (landing on `completed`, `failed`, or
`cancelled`). It **requires `store=True`** — the result has to live somewhere to be
retrieved — and is therefore **not compatible with Zero-Data-Retention** (contrast
the Responses-WebSocket mode above, whose warm cache is in-memory and ZDR-friendly).
It can be combined with `stream=True` to receive progress events as the job runs,
though the first event may be slower to arrive than on a foreground call.

```python
# Python — OpenAI Responses background job + poll loop
import time
from openai import OpenAI

client = OpenAI()

job = client.responses.create(
    model="gpt-5",
    input="Do a full architectural review of the attached repo.",
    background=True,        # returns immediately with an id
    store=True,             # REQUIRED for background; precludes ZDR
)
# job.id -> "resp_..."; job.status -> "queued"

while job.status in ("queued", "in_progress"):
    time.sleep(2)                              # poll cadence / backoff is on you
    job = client.responses.retrieve(job.id)    # survives client restarts: just re-retrieve by id

if job.status == "completed":
    print(job.output_text)
else:
    handle_failure(job.status, job.error)
```

```ts
// TS — OpenAI Responses background job + poll loop
import OpenAI from "openai";
const client = new OpenAI();

let job = await client.responses.create({
  model: "gpt-5",
  input: "Do a full architectural review of the attached repo.",
  background: true,   // returns immediately with an id
  store: true,        // REQUIRED for background; precludes ZDR
});

while (job.status === "queued" || job.status === "in_progress") {
  await new Promise((r) => setTimeout(r, 2000));     // poll cadence / backoff is on you
  job = await client.responses.retrieve(job.id);     // re-retrieve by id after any disconnect
}

if (job.status === "completed") console.log(job.output_text);
else handleFailure(job.status, job.error);
```

For a **single** long call, this is the whole durability story: persist the id, poll,
survive disconnects. Chaining many such steps into a durable multi-step run is
orchestration, not transport — see `07-workflows-and-orchestration.md`. Where the
poller itself runs (a worker, a queue consumer, a serverless function) is a
deployment shape — see `05-cross-cutting.md`.

### What varies across providers

- **Availability.** Background/async-job is **not universal**. Most providers offer
  only sync + streaming; a fire-and-poll lifecycle for a single call is the exception,
  not the baseline.
- **How you learn it finished.** Three patterns coexist: **polling** (re-retrieve by
  id, as above), **streaming progress** (reconnect to the event stream of a running
  job), and **webhook callbacks** (the provider POSTs you when the job lands). Which
  are offered, and which is idiomatic, differs by provider.
- **Storage coupling.** The requirement that a background job be *stored* to be
  retrievable ties this lifecycle to the server-stored state model
  (`03-state-sessions-memory.md`) — which not every provider or deployment allows.

### What's hard

- **Store/ZDR tension.** Background jobs need `store=True` to be retrievable, but
  Zero-Data-Retention forbids exactly that storage. The two are mutually exclusive,
  so a ZDR deployment simply cannot use this lifecycle.
- **Polling cadence and backoff.** Too-frequent polling wastes calls and hits rate
  limits; too-sparse polling adds latency to the result. There is no push to react
  to (unless you wire up streaming or webhooks), so the client owns the timing.
- **Reconciling with your own infra.** A provider-side background job overlaps
  awkwardly with a client-side queue/retry layer: who owns the retry, who owns the
  idempotency key, what happens if you poll a job your own system already gave up on.
- **Surfacing status in a UI.** "Running… / done" needs the client to translate
  `queued` / `in_progress` / `completed` / `failed` into UI state and keep polling in
  the background — a state machine a plain blocking call never needs.

---

## What varies / what's hard — the callout

**What varies across the landscape:**

- **Who initiates a response** — the client (`response.create`, every HTTP call) vs
  the *server* (server VAD auto-replying). This single difference reshapes the whole
  control flow.
- **Turn detection** — server VAD vs manual push-to-talk vs (in plain HTTP) no such
  concept at all.
- **Conversation representation** — role/content message arrays (Chat) vs typed
  *items* (Realtime, Responses) vs turn-based `server_content` (Gemini Live).
- **Event-grammar cardinality and direction** — ~10 (Anthropic stream) to ~35
  (Responses SSE) event types; one-directional (SSE) vs full-duplex (WS/WebRTC).
- **Streaming framing** — SSE vs chunked-JSON-array, same latency profile.
- **Transport split** — WebRTC's separate media + data planes vs WebSocket's single
  JSON channel.
- **Audio formats and voice catalogs** — `pcm16`/`g711_*` vs Gemini's PCM rates;
  non-interchangeable voice names.
- **Recoverability** — Realtime loses everything on disconnect; Responses-by-id and
  client-owned history survive.
- **Two architectures for a voice agent** — a single speech-to-speech model vs a
  cascaded STT→LLM→TTS pipeline — with opposite latency/control/provider-choice
  tradeoffs.
- **Unified voice abstractions make TTS/STT/STS providers swappable**, but providers
  diverge on capabilities (not all do realtime), voices/speakers, and audio formats — so
  the abstraction leaks.

**What's hard for anything sitting in front of all of this:**

- **The request/response shape doesn't fit the bottom.** Concurrent duplex, 1:N
  responses-per-session, server-initiated events, and server-authoritative state
  mean the primitive has to be **event-shaped**, with single-shot as the degenerate
  case — not the other way around.
- **Bidirectionality is binary.** Either mid-call client→server sends are legal or
  they aren't; there is no halfway. It reads cleanly as a single capability flag,
  and it gates which use cases a provider can even attempt.
- **Two orthogonal axes** — transport *and* state location — must both be modeled;
  reading one off the other gives wrong answers.
- **Normalizing event grammars loses information.** ~25 typed events per protocol,
  provider-specific, are better exposed as a typed-but-provider-specific union than
  flattened into a lowest-common-denominator shape.
- **Streaming audio escapes the "result" type.** Chunked PCM arriving every ~50ms
  has no home in a complete-artifact media value; it needs a live side channel, plus
  played-position bookkeeping to support barge-in truncation.
- **Concurrency and backpressure** — pumping input while draining output over one
  connection, indefinitely, with the connection buffering when the client falls
  behind — are problems that simply do not arise for a one-shot POST.
