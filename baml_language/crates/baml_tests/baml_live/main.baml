// The demo: a restaurant's phone line, as one LLM function.
//
// `ReservationLine` is an ordinary spec-mode function: a prompt, two tools,
// and a typed result. Calling it directly would run it through `ai.Agent` as
// text. The entry points below run the same spec through `voice.VoiceAgent`,
// which holds a spoken conversation and returns the typed result when the
// model ends the call.

class CallOutcome {
    /// True only when book_table confirmed the reservation.
    booked: bool,
    confirmation_code: string?,
    guest_name: string?,
    party_size: int?,
    day: string?,
    time: string?,
    /// One sentence on how the call went.
    summary: string,
}

class Availability {
    available: bool,
    alternatives: string[],
}

class Booking {
    confirmation_code: string,
    table: string,
}

/// Check whether a table is free. `day` is a weekday or date as the caller
/// said it, `time` is like "7:00 PM".
function check_availability(day: string, time: string, party_size: int) -> Availability {
    // The demo restaurant is full at 8 PM and cannot seat more than eight.
    if (party_size > 8 || time.to_lower_case().starts_with("8")) {
        Availability { available: false, alternatives: ["6:30 PM", "7:00 PM", "9:15 PM"] }
    } else {
        Availability { available: true, alternatives: [] }
    }
}

/// Book the table and return its confirmation code. A caller who asked for
/// the table has already confirmed it.
function book_table(guest_name: string, day: string, time: string, party_size: int) -> Booking {
    Booking {
        confirmation_code: `LG-${guest_name.length()}${party_size}${day.length()}${time.length()}`,
        table: if (party_size > 4) {
            "the long table by the window"
        } else {
            "a booth"
        },
    }
}

function ReservationLine(restaurant: string) -> CallOutcome {
    client: "openai/gpt-live-1"
    tools: [check_availability, book_table]
    prompt: `
        You answer the phone at ${restaurant}, an Italian restaurant, and take
        table reservations. Speak English. Keep every reply to one or two short
        sentences, the way a friendly host talks on the phone.

        To make a reservation you need the guest's name, the day, the time,
        and the party size. Ask for whatever is missing, one question at a
        time. Check availability before you promise a table. Once you have
        everything, book the table without asking the caller to confirm again,
        then tell the caller the confirmation code.

        A table is booked only when book_table returned a confirmation code.
        Read the caller that exact code. Never invent a code or say a table is
        booked before book_table succeeded.

        When the caller says goodbye or has nothing else, end the call.
    `
}

// ---------------------------------------------------------------------------
// Entry points

/// Prints the conversation as it happens, with seconds since the call began.
class Captions {
    speaker: string,
    started: baml.time.Instant,

    function _at(self) -> string throws never {
        let ms = self.started.elapsed().to_milliseconds().to_int() catch_all (e) {
            _ => 0,
        };
        `${ms / 1000}.${(ms % 1000) / 100}s`
    }

    function show(self, ev: voice.VoiceEvent) -> void throws never {
        match (ev) {
            let said: voice.UserSaid => self._say("you", said.delta),
            let said: voice.AssistantSaid => self._say("assistant", said.delta),
            let requested: voice.ToolCalls => {
                for (let call in requested.calls) {
                    let args = baml.json.to_string(call.args) catch_all (e) {
                        _ => "",
                    };
                    baml.io.println(`\n[${self._at()}]   tool: ${call.name} ${args}`);
                }
                self.speaker = "";
            },
            let reply: voice.BackendReply => {
                baml.io.println(`\n[${self._at()}]   backend: ${reply.text}`);
                self.speaker = "";
            },
            let fault: voice.SessionError => {
                baml.io.println(`\n[${self._at()}]   error: ${fault.message}`);
                self.speaker = "";
            },
            let ended: voice.Closed => {
                baml.io.println(`\n[${self._at()}]   closed: ${ended.reason}`);
            },
            _ => {},
        }
    }

    function _say(self, speaker: string, delta: string) -> void throws never {
        if (self.speaker != speaker) {
            baml.io.print(`\n[${self._at()}] ${speaker}: `);
            self.speaker = speaker;
        }
        baml.io.print(delta);
    }
}

function converse(
    provider: voice.VoiceProvider?,
    audio: voice.AudioDevice,
    max_duration_ms: int = 600000,
    on_event: ((ai.events.Event) -> void)? = null,
) -> ai.RunResult<CallOutcome> {
    let captions = Captions { speaker: "", started: baml.time.Instant.now() };
    let agent = voice.VoiceAgent.new(audio = audio, provider = provider, greeting = "Greet the caller: thank them for calling Luigi's and ask how you can help.", max_duration_ms = max_duration_ms, on_event = on_event, on_voice = (
        ev: voice.VoiceEvent,
    ) -> void {
        captions.show(ev)
    });
    agent.run(ReservationLine@spec("Luigi's"))
}

/// Talk to the reservation line on this Mac's microphone and speakers, on GPT-Live.
function talk() -> CallOutcome {
    baml.io.println(
        "Connecting to GPT-Live. Speak when the assistant greets you; say goodbye to end the call.",
    );
    converse(null, voice.MacAudioDevice.new()).value
}

/// The same conversation on the OpenAI Realtime API.
function talk_realtime() -> CallOutcome {
    baml.io.println(
        "Connecting to the Realtime API. Speak when the assistant greets you; say goodbye to end the call.",
    );
    converse(voice.OpenAIRealtime.new(transcription_language = "en"), voice.MacAudioDevice.new()).value
}

/// What `mic_check` measured.
class MicCheck {
    microphone_frames: int,
    /// The loudest sample seen, from 0 to 32767. Near zero means the
    /// microphone is muted or the terminal lacks microphone permission.
    peak_level: int,
    played_ms: int,
}

/// Checks the Mac audio device without a provider session: records the
/// microphone for about three seconds while playing a spoken phrase.
function mic_check(echo_cancellation: bool = true) -> MicCheck {
    let phrase = voice.synthesize("This is the audio check for the BAML voice demo.");
    let device = voice.MacAudioDevice.new(echo_cancellation = echo_cancellation);
    let frames = 0;
    let peak = 0;
    let heard_ms = 0;
    let played = false;
    while (heard_ms < 3000) {
        // A helper that exited, for example without microphone permission,
        // ends the input. Without this the loop would never advance.
        let frame: uint8array = device.next_input() else {
            break;
        };
        frames = frames + 1;
        heard_ms = heard_ms + frame.length() / 48;
        let i = 1;
        while (i < frame.length()) {
            let high = frame.at(i) ?? 0;
            let level = if (high > 127) {
                (255 - high) * 256
            } else {
                high * 256
            };
            if (level > peak) {
                peak = level;
            }
            i = i + 32;
        }
        if (!played) {
            device.play(phrase);
            played = true;
        }
    }
    device.drain();
    device.close();
    MicCheck { microphone_frames: frames, peak_level: peak, played_ms: phrase.length() / 48 }
}

/// What one scripted conversation produced.
class SelftestReport {
    provider: string,
    outcome: CallOutcome,
    /// The tools the model called, in order.
    tools_called: string[],
    /// True when the reported confirmation code is the one book_table returned.
    code_matches_booking: bool,
    /// The conversation as the journal recorded it.
    conversation: string[],
    /// What the assistant said, transcribed from the recorded audio by a
    /// separate speech-to-text model.
    assistant_audio_transcript: string,
    recording: string,
}

/// The caller's side of the scripted conversation. The caller thanks the
/// host only after the table is booked.
function caller_script() -> voice.ScriptedLine[] {
    [
        voice.ScriptedLine {
            pcm: voice.synthesize(
                "Hi, I'd like to book a table for two people tomorrow at seven P M. The name is Maria.",
            ),
            after_cue: null,
        },
        voice.ScriptedLine {
            pcm: voice.synthesize("Yes, that's right. Please go ahead and book it."),
            after_cue: null,
        },
        voice.ScriptedLine {
            pcm: voice.synthesize("That's great, thank you. That's all I needed. Goodbye!"),
            after_cue: "booked",
        },
    ]
}

function scripted_call(
    provider: voice.VoiceProvider,
    lines: voice.ScriptedLine[],
) -> SelftestReport {
    let caller = voice.ScriptedAudioDevice.new(lines);
    // The journal reports a finished tool by call id, so the names are kept
    // from the requests.
    let names: map<string, string> = {};
    let booking_outputs: string[] = [];
    let result = converse(provider, caller, max_duration_ms = 150000, on_event = (e: ai.events.Event) -> void {
        match (e) {
            let requested: ai.events.ToolRequested => {
                let _ = names.set(requested.id, requested.name);
            },
            let completed: ai.events.ToolCompleted => {
                if (names.get(completed.id) == "book_table") {
                    booking_outputs.push(completed.text());
                    caller.cue("booked");
                }
            },
            _ => {},
        }
    });
    let tools_called: string[] = [];
    let conversation: string[] = [];
    for (let e in result.journal.entries()) {
        match (e) {
            let requested: ai.events.ToolRequested => {
                tools_called.push(requested.name);
            },
            let message: ai.events.UserMessage => {
                conversation.push(`user: ${message.text()}`);
            },
            let message: ai.events.AssistantMessage => {
                for (let block in message.content) {
                    if let spoken: ai.content.Text = block {
                        conversation.push(`assistant: ${spoken.text}`);
                    }
                }
            },
            _ => {},
        }
    }
    let recorded = caller.recorded();
    let recording = `${voice.scratch_dir()}/selftest_${provider.id().replace("/", "_")}.wav`;
    let _ = baml.fs.write_bytes(recording, voice.wav(recorded));
    SelftestReport {
        provider: provider.id(),
        outcome: result.value,
        tools_called: tools_called,
        code_matches_booking: booking_outputs.some((output: string) -> bool {
            output.includes(result.value.confirmation_code ?? "no code")
        }),
        conversation: conversation,
        assistant_audio_transcript: voice.transcribe(recorded),
        recording: recording,
    }
}

/// No microphone needed. Synthesizes the caller with text to speech, holds the
/// conversation on both providers, and reports what each one did.
function selftest() -> SelftestReport[] {
    let lines = caller_script();
    [
        scripted_call(voice.OpenAILive.new(), lines),
        scripted_call(voice.OpenAIRealtime.new(transcription_language = "en"), lines),
    ]
}

function selftest_live() -> SelftestReport {
    scripted_call(voice.OpenAILive.new(), caller_script())
}

function selftest_realtime() -> SelftestReport {
    scripted_call(voice.OpenAIRealtime.new(transcription_language = "en"), caller_script())
}
