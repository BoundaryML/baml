// High-level realtime runner. Unlike Realtime, which returns an open
// LiveSession, VoiceAgent owns the session loop and audio device lifecycle.
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
            let live_session = task.run(runner = Realtime.new(self.channel));
            let microphone_pump = spawn { pump_voice_input(self.audio, live_session) };

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
            let barge_in_timers: baml.future.Future<null, null>[] = [];
            let completion_call_ids: string[] = [];
            defer {
                state.closed = true;
                state.user_speaking = false;
                microphone_pump.cancel();
                for (let timer in barge_in_timers) {
                    timer.cancel();
                }
                live_session.close();
                self.audio.close();
            }
            while (!closed) {
                let events = live_session.receive();
                if (events.length() == 0) {
                    self.audio.finish_output();
                    closed = true;
                }
                for (let event in events) {
                    log.info(event);
                    match (event) {
                        let started: root.ai.UserSpeechStarted => {
                            // OpenAI server VAD is the source of truth for whether the
                            // microphone contains speech. Brief detections do not barge in:
                            // speech_stopped invalidates this timer before it fires.
                            state.user_speaking = true;
                            state.user_speech_item_id = started.item_id;
                            state.speech_generation = state.speech_generation + 1;
                            let speech_generation = state.speech_generation;
                            barge_in_timers.push(spawn {
                                interrupt_after_sustained_user_speech(
                                    state,
                                    speech_generation,
                                    self.barge_in_after_ms,
                                    self.audio,
                                    live_session,
                                )
                            });
                        },
                        let stopped: root.ai.UserSpeechStopped => {
                            if (state.user_speech_item_id == stopped.item_id) {
                                state.user_speaking = false;
                                state.user_speech_item_id = null;
                                if (state.assistant_response_active) {
                                    // The committed user turn waits until the current
                                    // response reaches its OpenAI completion boundary.
                                    state.user_turn_pending = true;
                                } else {
                                    live_session.request_response();
                                    state.user_turn_pending = false;
                                }
                            }
                        },
                        let started: root.ai.AssistantAudioStarted => {
                            // Assistant audio is latest-wins. A newly-started
                            // assistant item replaces any older queued playback.
                            if (state.assistant_audio_active) {
                                self.audio.stop_output();
                            }
                            state.assistant_audio_active = true;
                            state.assistant_response_active = true;
                        },
                        let delta: root.ai.AssistantAudioDelta => {
                            self.audio.play_output(delta.audio);
                        },
                        let transcript: root.ai.UserTranscript => {
                            baml.io.println("\nYou: " + transcript.text);
                        },
                        let delta: root.ai.TranscriptDelta => {
                            baml.io.print(delta.text);
                        },
                        let calls: root.ai.LiveToolCalls => {
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
                            self.audio.finish_output();
                            state.assistant_response_active = false;
                            if (state.user_turn_pending && !state.user_speaking) {
                                state.user_turn_pending = false;
                                live_session.request_response();
                            }
                        },
                        let ended: root.ai.LiveClosed => {
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

function interrupt_after_sustained_user_speech(
    state: VoiceAgentState,
    speech_generation: int,
    delay_ms: int,
    audio: root.ai.RealtimeAudioDevice,
    live_session: root.ai.LiveSession,
) -> null throws never {
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
        return null;
    }

    // With a WebSocket client, OpenAI cannot know how much locally-buffered
    // audio was heard. Stop it first, then cancel generation and truncate the
    // assistant item to the exact local playback position.
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

function pump_voice_input(
    audio: root.ai.RealtimeAudioDevice,
    live_session: root.ai.LiveSession,
) -> null throws never {
    let listening = true;
    while (listening) {
        let next_input = audio.next_input() catch_all (e) {
            _ => {
                log.debug({ "voice_agent_microphone_stopped": e });
                return null;
            }
        };
        match (next_input) {
            let pcm: uint8array => {
                live_session.send_audio(pcm) catch_all (e) {
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
                listening = false;
            },
        }
    }
    null
}
