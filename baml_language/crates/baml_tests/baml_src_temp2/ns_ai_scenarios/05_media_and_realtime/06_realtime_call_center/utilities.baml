// Scenario-specific call-center types, fake tools, and live audio utilities.
class ScheduledMeeting {
    confirmation_id: string,
    attendee_name: string,
    attendee_email: string,
    date: string,
    time: string,
    timezone: string,
    duration_minutes: int,
    purpose: string,
}

/// Fake scheduling backend. Call only after the caller explicitly confirms
/// every required meeting detail.
function schedule_meeting(
    attendee_name: string,
    attendee_email: string,
    date: string,
    time: string,
    timezone: string,
    duration_minutes: int,
    purpose: string,
) -> ScheduledMeeting throws never {
    let meeting = ScheduledMeeting {
        confirmation_id: "demo-" + baml.sys.now_ms().to_string(),
        attendee_name: attendee_name,
        attendee_email: attendee_email,
        date: date,
        time: time,
        timezone: timezone,
        duration_minutes: duration_minutes,
        purpose: purpose,
    };
    log.info({ "event": "fake_schedule_meeting", "arguments": meeting });
    baml.io.println("[fake tool] schedule_meeting");
    let encoded = baml.json.to_json(meeting) catch (e) {
        _ => null
    };
    baml.io.println(baml.json.stringify(encoded));
    meeting
}

class WeatherReport {
    location: string,
    date: string,
    conditions: string,
    temperature_fahrenheit: int,
    precipitation_chance_percent: int,
}

/// Fake weather backend for the voice-agent demo. It intentionally returns the
/// same canned forecast while preserving the location and date the caller asks
/// about.
function check_weather(location: string, date: string) -> WeatherReport throws never {
    let report = WeatherReport {
        location: location,
        date: date,
        conditions: "Sunny with a light breeze",
        temperature_fahrenheit: 72,
        precipitation_chance_percent: 5,
    };
    log.info(
        {
            "event": "fake_check_weather",
            "arguments": { "location": location, "date": date },
            "result": report,
        },
    );
    baml.io.println("[fake tool] check_weather");
    let encoded = baml.json.to_json(report) catch (e) {
        _ => null
    };
    baml.io.println(baml.json.stringify(encoded));
    report
}

class ConversationCompleted {
    reason: string,
}

/// Signal that the caller has explicitly finished the conversation. Do not
/// call this merely because a meeting was scheduled.
function mark_conversation_complete(reason: string) -> ConversationCompleted throws never {
    let completed = ConversationCompleted { reason: reason };
    log.info({ "event": "conversation_completed", "reason": reason });
    baml.io.println("[fake tool] mark_conversation_complete");
    completed
}

function _realtime_call_center_instructions() -> string throws never {
    "
  You are Maya, a concise and friendly call-center scheduling agent speaking
  with a caller over the phone.

  Before calling schedule_meeting, collect every one of these values:
  - caller's full name
  - caller's email address
  - calendar date
  - local start time
  - timezone (ask for their city or an IANA timezone if it is unclear)
  - duration in minutes
  - meeting purpose

  Never guess a missing value. Once all values are present, briefly repeat the
  complete meeting details and obtain an explicit confirmation. Only then call
  schedule_meeting, exactly once. Do not claim the meeting is scheduled until
  its tool result succeeds. After success, read the confirmation back naturally.
  Scheduling a meeting does not end the call. Ask whether the caller needs
  anything else and continue helping them.

  If the caller asks about weather, call check_weather with the requested
  location and date. This demo tool returns a canned forecast. Briefly tell the
  caller what it reports, then continue the scheduling conversation.

  Call mark_conversation_complete only after the caller explicitly says goodbye,
  asks to hang up, or clearly says they need nothing else. Give a brief natural
  farewell, then call mark_conversation_complete with the reason the call ended.
  Never use that tool merely because schedule_meeting succeeded.

  Keep each spoken response short and ask only for the next missing details.
  "
}

function _realtime_call_center_task(
    provider: root.ai.Provider,
    first_turn: string,
) -> root.ai.Task<null> throws never {
    root.ai.task_named<null>(provider, "RealtimeCallCenter", { "first_turn": first_turn }, [schedule_meeting, check_weather, mark_conversation_complete], (
        ctx: baml.llm.Context,
    ) -> baml.llm.PromptAst {
        baml.llm
            .assemble_prompt_ast(
                ["", "", "", "", ""],
                [
                    baml.llm.Role { name: "system", metadata: {} },
                    _realtime_call_center_instructions(),
                    baml.llm.Role { name: "user", metadata: {} },
                    first_turn,
                ],
            )
    })
}

function realtime_audio_helper_path() -> string throws baml.errors.UnknownError {
    let configured = baml.env.get("BAML_REALTIME_AUDIO_HELPER") catch (e) {
        _ => {
            throw baml.errors.UnknownError.with_message<never>(e, "could not read BAML_REALTIME_AUDIO_HELPER")
        },
    };
    configured ?? "crates/baml_tests/support/realtime_audio_macos.swift"
}

function realtime_openai_api_key() -> string throws baml.errors.UnknownError {
    let key = baml.env.get("OPENAI_API_KEY") catch (e) {
        _ => {
            throw baml.errors.UnknownError.with_message<never>(e, "could not read OPENAI_API_KEY")
        },
    };
    match (key) {
        let value: string => value,
        null => {
            throw baml.errors.UnknownError.with_message<never>(null, "OPENAI_API_KEY is required")
        },
    }
}

function start_realtime_audio_device() -> baml.sys.Process throws baml.errors.UnknownError {
    baml.sys
        .start_process(
            "/usr/bin/swift",
            [realtime_audio_helper_path(), "duplex"],
            baml.sys.ProcessOptions {
                cwd: null,
                env: null,
                timeout_ms: null,
                stdin: null,
                keep_stdin_open: true,
            },
        ) catch (e) {
        _ => {
            throw baml.errors.UnknownError.with_message<never>(e, "could not start full-duplex macOS audio")
        },
    }
}

// The reusable VoiceAgent runner depends only on RealtimeAudioDevice. This
// adapter supplies full-duplex macOS capture and playback from one
// voice-processing audio engine, preserving barge-in while cancelling echo.
class MacRealtimeAudioDevice {
    process: baml.sys.Process,
    playback_started_ms: int?,
    playback_bytes: int,
    output_finished: bool,
    is_closed: bool,

    function new() -> MacRealtimeAudioDevice throws baml.errors.UnknownError {
        MacRealtimeAudioDevice {
            process: start_realtime_audio_device(),
            playback_started_ms: null,
            playback_bytes: 0,
            output_finished: true,
            is_closed: false,
        }
    }

    implements root.ai.realtime.RealtimeAudioDevice {
        function input_format(self) -> root.ai.realtime.RealtimeAudioFormat throws never {
            root.openai.realtime_audio_format()
        }

        function output_format(self) -> root.ai.realtime.RealtimeAudioFormat throws never {
            root.openai.realtime_audio_format()
        }

        function next_input(self) -> uint8array? throws baml.errors.UnknownError {
            if (self.is_closed) {
                return null;
            }
            let next = self.process.stdout._next() catch (e) {
                _ => {
                    throw baml.errors.UnknownError.with_message<never>(e, "could not read a microphone frame")
                },
            };
            match (next) {
                let encoded: string => {
                    baml.Uint8Array.from_base64(encoded) catch (e) {
                        _ => {
                            throw baml.errors.UnknownError.with_message<never>(e, "microphone helper emitted an invalid audio frame")
                        },
                    }
                },
                null => null,
            }
        }

        function play_output(
            self,
            pcm16_24khz_mono: uint8array,
        ) -> null throws baml.errors.UnknownError {
            if (self.output_finished) {
                self.playback_started_ms = baml.sys.now_ms();
                self.playback_bytes = 0;
                self.output_finished = false;
            }
            self.process.write_stdin("audio " + pcm16_24khz_mono.to_base64() + "\n") catch (e) {
                _ => {
                    throw baml.errors.UnknownError.with_message<never>(e, "could not send assistant audio to the macOS audio device")
                },
            };
            self.playback_bytes = self.playback_bytes + pcm16_24khz_mono.length();
            null
        }

        function finish_output(self) -> null throws never {
            self.output_finished = true;
            null
        }

        function stop_output(self) -> int throws baml.errors.UnknownError {
            // PCM16 mono at 24 kHz is 48 bytes per millisecond.
            let playback_duration_ms = self.playback_bytes / 48;
            let heard_ms = match (self.playback_started_ms) {
                let started: int => {
                    let elapsed = baml.sys.now_ms() - started;
                    if (elapsed < 0) {
                        0
                    } else if (elapsed > playback_duration_ms) {
                        playback_duration_ms
                    } else {
                        elapsed
                    }
                },
                null => 0,
            };
            self.process.write_stdin("stop\n") catch (e) {
                _ => {
                    throw baml.errors.UnknownError.with_message<never>(e, "could not stop macOS assistant audio playback")
                },
            };
            self.playback_started_ms = null;
            self.playback_bytes = 0;
            self.output_finished = true;
            heard_ms
        }

        function close(self) -> null throws never {
            if (!self.is_closed) {
                self.process.close_stdin() catch_all (e) {
                    _ => null
                };
                self.process.close();
                self.is_closed = true;
            }
            null
        }
    }
    implements root.ai.Resource {}
}

// Run from the repository root:
//   baml run ai_scenarios.openai_realtime_call_center \
//     --from crates/baml_tests/baml_src_temp2
function openai_realtime_call_center() -> null throws root.ai.Failure | baml.errors.UnknownError {
    let provider = root.openai.Realtime {
        model: "gpt-realtime-2.1",
        voice: "marin",
        api_key: realtime_openai_api_key(),
        output_audio: true,
        input_transcription_model: "gpt-4o-mini-transcribe",
        server_vad: root.openai.RealtimeServerVad {
            threshold: 0.5,
            prefix_padding_ms: 300,
            silence_duration_ms: 500,
            // Keep OpenAI VAD's speech boundary events and automatic input
            // commits, but let VoiceAgent own response timing and barge-in.
            create_response: false,
            // VoiceAgent applies the 500 ms sustained-speech policy and
            // then cancels, stops local playback, and truncates explicitly.
            interrupt_response: false,
        },
        keep_open: true,
    };
    let channel = root.ai.testing.RecordingChannel { frames: [] };
    let task = _realtime_call_center_task(provider, "Begin the call now: introduce yourself and ask what meeting I want to schedule.");
    baml.io
        .println(
            "\nMicrophone is live with macOS echo cancellation. OpenAI server VAD detects each turn; press Ctrl+C to hang up.",
        );
    task.run(
        runner = root
            .ai
            .run
            .VoiceAgent
            .new(
                MacRealtimeAudioDevice.new(),
                channel,
                completion_tool = "mark_conversation_complete",
                barge_in_after_ms = 500,
            ),
    );
    baml.io.println("Call ended.");
    null
}
