// High-level realtime runner. Raw provider sessions are opened with
// root.ai.open_live; VoiceAgent adds the managed event loop, application-tool
// execution, and audio device lifecycle.
class VoiceAgent {
    audio: root.ai.RealtimeAudioDevice,
    channel: root.ai.Channel,
    completion_tool: string?,
    barge_in_after_ms: int,

    function new(
        audio: root.ai.RealtimeAudioDevice,
        channel: root.ai.Channel,
        completion_tool: string? = null,
        barge_in_after_ms: int = 500,
    ) -> VoiceAgent throws never {
        VoiceAgent {
            audio: audio,
            channel: channel,
            completion_tool: completion_tool,
            barge_in_after_ms: barge_in_after_ms,
        }
    }

    implements root.ai.Runner<root.ai.Task<null>> {
        type Output = null
        type Error = baml.errors.UnknownError | baml.errors.Unsupported

        function run(
            self,
            task: root.ai.Task<null>,
        ) -> null throws baml.errors.UnknownError | baml.errors.Unsupported {
            //# Open the raw provider-owned realtime resource
            let raw_live_session = root.ai.open_live(task, self.channel);
            //# Add application tool execution only for the managed voice lifecycle
            let live_session = if (task.tools.length() > 0) {
                root.ai
                    .with_automatic_tools(
                        raw_live_session,
                        root.ai._tool_inputs_from_tools(task.tools),
                    )
            } else {
                raw_live_session
            };
            //# Verify the audio device and provider session negotiated the same formats
            let device_input = self.audio.input_format();
            let session_input = live_session.input_audio_format();
            if (!device_input.is_compatible_with(session_input)) {
                live_session.close();
                self.audio.close();
                throw baml.errors.Unsupported {
                    message: `voice input format mismatch: device=${device_input.describe()}, session=${session_input.describe()}`,
                };
            }
            let device_output = self.audio.output_format();
            let session_output = live_session.output_audio_format();
            if (!device_output.is_compatible_with(session_output)) {
                live_session.close();
                self.audio.close();
                throw baml.errors.Unsupported {
                    message: `voice output format mismatch: device=${device_output.describe()}, session=${session_output.describe()}`,
                };
            }
            //# Pump microphone audio concurrently with provider event handling
            let microphone_pump = spawn { _pump_voice_input(self.audio, live_session) };

            let closed = false;
            let state = VoiceAgentState {
                assistant_audio_active: false,
                assistant_response_active: false,
                user_speaking: false,
                user_speech_item_id: null,
                user_turn_pending: false,
                speech_generation: 0,
                closed: false,
            };
            let barge_in_timers: baml.future.Future<null, never>[] = [];
            let completion_call_ids: string[] = [];
            //# Always stop spawned work and close audio and provider resources
            defer {
                state.closed = true;
                state.user_speaking = false;
                //## Cancel the microphone and pending barge-in timers
                microphone_pump.cancel();
                for (let timer in barge_in_timers) {
                    timer.cancel();
                }
                //## Close the realtime session and local audio device
                live_session.close();
                self.audio.close();
            }
            //# Process realtime event batches until the call closes
            while (!closed) {
                //## Receive the next provider event batch
                let events = live_session.receive();
                if (events.length() == 0) {
                    self.audio.finish_output();
                    closed = true;
                }
                //## Apply every event to the voice-agent state machine
                for (let event in events) {
                    log.info(event);
                    match (event) {
                        let started: root.ai.UserSpeechStarted => {
                            //### Arm delayed interruption when the caller begins speaking
                            // Provider-side VAD is the source of truth for whether the
                            // microphone contains speech. Brief detections do not barge
                            // in: speech_stopped invalidates this timer before it fires.
                            state.user_speaking = true;
                            state.user_speech_item_id = started.item_id;
                            state.speech_generation = state.speech_generation + 1;
                            let speech_generation = state.speech_generation;
                            barge_in_timers.push(spawn {
                                _interrupt_after_sustained_user_speech(
                                    state,
                                    speech_generation,
                                    self.barge_in_after_ms,
                                    self.audio,
                                    live_session,
                                )
                            });
                        },
                        let stopped: root.ai.UserSpeechStopped => {
                            //### Request a response when the caller finishes a turn
                            if (state.user_speech_item_id == stopped.item_id) {
                                state.user_speaking = false;
                                state.user_speech_item_id = null;
                                if (state.assistant_response_active) {
                                    // The committed user turn waits until the current
                                    // response reaches the provider's completion boundary.
                                    state.user_turn_pending = true;
                                } else {
                                    live_session.request_response();
                                    state.user_turn_pending = false;
                                }
                            }
                        },
                        let started: root.ai.AssistantAudioStarted => {
                            //### Replace queued playback with the newest assistant response
                            // Assistant audio is latest-wins. A newly-started
                            // assistant item replaces any older queued playback.
                            if (state.assistant_audio_active) {
                                self.audio.stop_output();
                            }
                            state.assistant_audio_active = true;
                            state.assistant_response_active = true;
                        },
                        let delta: root.ai.AssistantAudioDelta => {
                            //### Play the next assistant audio frame
                            self.audio.play_output(delta.audio);
                        },
                        let transcript: root.ai.UserTranscript => {
                            baml.io.println("\nYou: " + transcript.text);
                        },
                        let delta: root.ai.TranscriptDelta => {
                            baml.io.print(delta.text);
                        },
                        let calls: root.ai.LiveToolCalls => {
                            //### Remember calls that explicitly complete the voice workflow
                            match (self.completion_tool) {
                                let completion_tool: string => {
                                    for (let call in calls.calls) {
                                        if (call.name == completion_tool) {
                                            completion_call_ids.push(call.id);
                                        }
                                    }
                                },
                                null => {},
                            }
                        },
                        let results: root.ai.LiveToolResults => {
                            //### Close after a successful completion-tool result
                            if (
                                results.results.some((result: root.ai.ToolResult) -> bool {
                                    completion_call_ids.includes(result.id) && !result.is_error
                                })
                            ) {
                                // A completion tool call is the response boundary, so flush
                                // any farewell audio even though no LiveTurnCompleted follows.
                                self.audio.finish_output();
                                closed = true;
                            }
                        },
                        let completed: root.ai.LiveTurnCompleted => {
                            //### Finish playback and release any queued caller turn
                            self.audio.finish_output();
                            state.assistant_response_active = false;
                            if (state.user_turn_pending && !state.user_speaking) {
                                state.user_turn_pending = false;
                                live_session.request_response();
                            }
                        },
                        let ended: root.ai.LiveClosed => {
                            //### Stop when the provider closes the live session
                            self.audio.finish_output();
                            closed = true;
                        },
                        _ => {},
                    }
                }
            }

            null
        }
    }
}

class VoiceAgentState {
    assistant_audio_active: bool,
    assistant_response_active: bool,
    user_speaking: bool,
    user_speech_item_id: string?,
    user_turn_pending: bool,
    speech_generation: int,
    closed: bool,
}

function _interrupt_after_sustained_user_speech(
    state: VoiceAgentState,
    speech_generation: int,
    delay_ms: int,
    audio: root.ai.RealtimeAudioDevice,
    live_session: root.ai.LiveSession,
) -> null throws never {
    //# Wait until caller speech has lasted long enough to count as barge-in
    baml.sys.sleep(baml.time.Duration.from_milliseconds(delay_ms)) catch (e) {
        _ => {
            return null;
        }
    };
    if (
        state.closed
            || !state.user_speaking
            || state.speech_generation != speech_generation
            || !state.assistant_audio_active
    ) {
        //## Ignore stale timers and speech that ended before the threshold
        return null;
    }
    //# Stop local playback before cancelling and truncating remote generation
    // A remote session cannot know how much locally-buffered audio was heard.
    // Stop it first, then cancel generation and truncate the assistant item to
    // the exact local playback position.
    let played_ms = audio.stop_output() catch_all (e) {
        _ => {
            log.info({ "voice_agent_barge_in_error": e });
            return null;
        }
    };
    live_session.cancel_response() catch_all (e) {
        _ => {
            log.info({ "voice_agent_barge_in_error": e });
            return null;
        }
    };
    live_session.truncate_assistant_audio(played_ms) catch_all (e) {
        _ => {
            log.info({ "voice_agent_barge_in_error": e });
            return null;
        }
    };
    state.assistant_audio_active = false;
    baml.io
        .println(
            "\nAssistant interrupted after "
                + delay_ms.to_string()
                + " ms of continuous user speech.",
        );
    null
}

function _pump_voice_input(
    audio: root.ai.RealtimeAudioDevice,
    live_session: root.ai.LiveSession,
) -> null throws never {
    let listening = true;
    //# Forward microphone frames until input ends or the live session closes
    while (listening) {
        //## Read the next frame from the audio device
        let next_input = audio.next_input() catch_all (e) {
            _ => {
                log.debug({ "voice_agent_microphone_stopped": e });
                return null;
            }
        };
        match (next_input) {
            let audio_frame: uint8array => {
                //## Send captured audio into the provider session
                live_session.send_audio(audio_frame) catch_all (e) {
                    _ => {
                        // The receive loop owns session errors. If it closes the
                        // session, the microphone pump should stop without
                        // producing a second unhandled spawned-task failure.
                        log.debug({ "voice_agent_microphone_stopped": e });
                        return null;
                    }
                };
            },
            null => {
                //## Stop when the audio device reaches end-of-input
                listening = false;
            },
        }
    }
    null
}
