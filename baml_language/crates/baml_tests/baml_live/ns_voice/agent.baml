// VoiceAgent: an `ai.Runner` that executes a spec as a spoken conversation.
//
// `ai.Agent` runs a spec as a request/response loop against an `ai.Client`.
// VoiceAgent runs the same spec against a realtime speech session:
//
//   the spec's prompt     becomes the voice model's instructions
//   the spec's toolbox    is what the model can do during the conversation
//   the spec's output     is the argument of a `finish_conversation` tool the
//                         runner adds; calling it ends the run with that value
//
// The journal records the conversation in the usual events: transcribed user
// and assistant speech as messages, tool calls with their results, and the
// final value.

/// The session ended before the model reported a result.
class SessionEnded {
    reason: string,
}

/// (internal) State shared by the green threads of one run.
class RunState {
    closed: bool,
    timed_out: bool,
    // Set once `finish_conversation` was accepted; the farewell follows.
    finished: bool,
    final_json: string,
    spoke_since_finish: bool,
    // When the assistant last produced speech, and when anyone last spoke.
    last_speech_ms: int,
    last_activity_ms: int,
    finished_at_ms: int,
    close_reason: string,
    // The speaker of the transcript being accumulated: "user", "assistant", or "".
    speaker: string,
    text: string,
}

function finish_tool_name() -> string {
    "finish_conversation"
}

/// The tool whose argument is the spec's output value.
function finish_tool(output_type: reflect.Type) -> ai.tools.Tool {
    let result_schema = baml.json.schema(output_type) catch_all (e) {
        _ => {
            let any: baml.json.json = {};
            any
        },
    };
    let schema: map<string, unknown> = {
        "type": "object",
        "properties": { "result": result_schema },
        "required": ["result"],
        "additionalProperties": false,
    };
    ai.tools.raw_tool(finish_tool_name(), "End the conversation and report its outcome. Call this once the user has said goodbye or made clear they need nothing else. `result` is the outcome of the whole conversation.", schema, (
        args: map<string, unknown>,
    ) -> string {
        "ok"
    })
}

class VoiceAgent {
    audio: AudioDevice,
    /// Null selects the provider from the spec's `client:` line:
    /// `openai/gpt-live-*` or `openai/gpt-realtime*`.
    provider: VoiceProvider?,
    /// What the assistant says to open the conversation. Null waits for the user.
    greeting: string?,
    max_duration_ms: int,
    /// The run ends with `baml.errors.Timeout` when nobody has spoken for this long.
    idle_timeout_ms: int,
    // How long the assistant may take to say goodbye after it finished.
    farewell_ms: int,
    on_event: ((ai.events.Event) -> void throws never)?,
    on_voice: ((VoiceEvent) -> void throws never)?,

    function new(
        audio: AudioDevice,
        provider: VoiceProvider? = null,
        greeting: string? = null,
        max_duration_ms: int = 600000,
        idle_timeout_ms: int = 60000,
        farewell_ms: int = 20000,
        on_event: ((ai.events.Event) -> void)? = null,
        on_voice: ((VoiceEvent) -> void throws never)? = null,
    ) -> VoiceAgent {
        VoiceAgent {
            audio: audio,
            provider: provider,
            greeting: greeting,
            max_duration_ms: max_duration_ms,
            idle_timeout_ms: idle_timeout_ms,
            farewell_ms: farewell_ms,
            on_event: ai.events.guard(on_event),
            on_voice: on_voice,
        }
    }

    /// (internal) Append to the journal and report to `on_event`.
    function _record(self, j: ai.Journal, events: ai.events.Event[]) -> void throws never {
        j.append_all(events);
        if let cb: (ai.events.Event) -> void throws never = self.on_event {
            for (let e in events) {
                cb(e);
            }
        }
    }

    /// (internal) Commit the transcript accumulated so far as one message.
    function _flush_transcript(
        self,
        j: ai.Journal,
        state: RunState,
        client_id: string,
    ) -> void throws never {
        let text = state.text.trim();
        if (text != "") {
            if (state.speaker == "user") {
                self._record(j, [ai.events.UserMessage.new(text)]);
            } else {
                self._record(
                    j,
                    [
                        ai.events.AssistantMessage {
                            content: [ai.content.Text { text: text }],
                            client_id: client_id,
                        },
                    ],
                );
            }
        }
        state.speaker = "";
        state.text = "";
    }

    /// (internal) Transcripts arrive as fragments. A change of speaker ends a message.
    function _hear(
        self,
        j: ai.Journal,
        state: RunState,
        client_id: string,
        speaker: string,
        delta: string,
    ) -> void throws never {
        if (state.speaker != speaker) {
            self._flush_transcript(j, state, client_id);
            state.speaker = speaker;
        }
        state.text = state.text + delta;
    }

    /// (internal) Stream the microphone into the session until either side ends.
    function _pump_microphone(self, session: VoiceSession, state: RunState) -> void throws never {
        while (!state.closed) {
            let frame = self.audio.next_input() catch_all (e) {
                _ => null,
            };
            if let pcm: uint8array = frame {
                if (pcm.length() > 0 && !state.closed) {
                    session.send_audio(pcm) catch_all (e) {
                        _ => {
                            state.closed = true;
                        },
                    };
                }
            } else {
                // The input ended. The conversation continues without it.
                return;
            }
        }
    }

    /// (internal) Close the session after `delay_ms` unless the run ended first.
    function _close_after(
        self,
        session: VoiceSession,
        state: RunState,
        delay_ms: int,
        timed_out: bool,
    ) -> void throws never {
        baml.sys.sleep(baml.time.Duration.from_milliseconds(delay_ms)) catch_all (e) {
            _ => null,
        };
        if (!state.closed) {
            if (timed_out) {
                state.timed_out = true;
            }
            session.close();
        }
    }

    /// (internal) Close the session once nobody has spoken for `idle_timeout_ms`.
    function _watch_idle(
        self,
        session: VoiceSession,
        state: RunState,
        clock: PlaybackClock,
    ) -> void throws never {
        while (!state.closed) {
            baml.sys.sleep(baml.time.Duration.from_seconds(1)) catch_all (e) {
                _ => null,
            };
            if (!state.closed && clock.now_ms() - state.last_activity_ms > self.idle_timeout_ms) {
                state.timed_out = true;
                session.close();
                return;
            }
        }
    }

    /// (internal) Run the tools of one `ToolCalls` event and deliver every result
    /// together. `done` holds the results the runner already produced.
    function _run_tools(
        self,
        toolbox: ai.tools.Toolbox,
        calls: ai.content.ToolUse[],
        done: ToolResult[],
        j: ai.Journal,
        session: VoiceSession,
        state: RunState,
    ) -> void throws never {
        let results = done;
        for (let call in calls) {
            let output = match (toolbox.get(call.name)) {
                let t: ai.tools.Tool => {
                    t.call(call.args) catch_all (e) {
                        _ => {
                            self._record(
                                j,
                                [ai.events.ToolFailed { id: call.id, message: e.to_string() }],
                            );
                            // The failure is the call's result, so the model can adapt.
                            `error: ${e.to_string()}`
                        },
                    }
                },
                null => {
                    self._record(
                        j,
                        [
                            ai.events.ToolFailed {
                                id: call.id,
                                message: `tool is not in the active toolbox: ${call.name}`,
                            },
                        ],
                    );
                    `error: there is no tool named ${call.name}`
                },
            };
            if (!output.starts_with("error: ")) {
                self._record(j, [ai.events.ToolCompleted.new(call.id, output)]);
            }
            results.push(ToolResult { id: call.id, output: output });
        }
        if (!state.closed) {
            session.send_tool_results(results) catch_all (e) {
                _ => {
                    state.close_reason = `could not deliver tool results: ${e.to_string()}`;
                    session.close();
                },
            };
        }
    }

    implements ai.Runner {
        type Error = baml.errors.Io
            | baml.errors.Timeout
            | baml.errors.InvalidArgument
            | reflect.errors.CompilationError
            | ai.errors.PromptRenderError
            | ai.errors.InvalidRequest
            | SessionEnded;

        function run<Out>(self, spec: ai.FunctionSpec<Out>) -> ai.RunResult<Out> {
            let provider = self.provider ?? provider_for(spec.client_id());
            let selected: VoiceProvider = provider else {
                throw baml.errors.InvalidArgument {
                    message: `${spec.client_id()} is not a realtime voice model; use client: "openai/gpt-live-1" or "openai/gpt-realtime", or pass a provider`,
                }
            };
            let client_id = selected.id();
            let finish = finish_tool(spec.output_type());
            let tools: ai.tools.Tool[] = [];
            for (let t in spec.tools().list()) {
                tools.push(t);
            }
            tools.push(finish);
            let instructions = spec.prompt().text()
                + `\n\nThe conversation ends only when the user says goodbye or makes clear they need nothing else. Then say a short goodbye and end the conversation with ${finish_tool_name()}.`;

            let j = ai.Journal.new(spec);
            if let cb: (ai.events.Event) -> void throws never = self.on_event {
                for (let e in j.entries()) {
                    cb(e);
                }
            }
            let total = ai.events.Usage {
                input_tokens: 0,
                output_tokens: 0,
                cached_input_tokens: null,
                reasoning_tokens: null,
            };
            let state = RunState {
                closed: false,
                timed_out: false,
                finished: false,
                final_json: "",
                spoke_since_finish: false,
                last_speech_ms: 0,
                last_activity_ms: 0,
                finished_at_ms: 0,
                close_reason: "",
                speaker: "",
                text: "",
            };
            // Also tracks how much of the current assistant item has been
            // heard, which a truncating provider needs on interruption.
            let clock = PlaybackClock.new();
            let playing_item: string? = null;

            let session = selected.connect(SessionConfig { instructions: instructions, tools: tools });
            let microphone = spawn { self._pump_microphone(session, state) };
            let timers = [
                spawn { self._close_after(session, state, self.max_duration_ms, true) },
                spawn { self._watch_idle(session, state, clock) },
            ];
            if let hello: string = self.greeting {
                session.say(hello);
            }

            while (!state.closed) {
                let events = session.next_events() catch_all (e) {
                    _ => {
                        let failed: VoiceEvent[] = [Closed { reason: e.to_string() }];
                        failed
                    },
                };
                for (let ev in events) {
                    if let cb: (VoiceEvent) -> void throws never = self.on_voice {
                        cb(ev);
                    }
                    match (ev) {
                        let a: AssistantAudio => {
                            clock.queue(a.pcm.length(), new_utterance = a.item_id != playing_item);
                            playing_item = a.item_id;
                            if (!is_silent(a.pcm)) {
                                state.last_speech_ms = clock.now_ms();
                                state.last_activity_ms = state.last_speech_ms;
                                if (state.finished) {
                                    state.spoke_since_finish = true;
                                }
                            }
                            self.audio.play(a.pcm) catch_all (e) {
                                _ => {
                                    state.close_reason = `audio playback failed: ${e.to_string()}`;
                                    session.close();
                                },
                            };
                        },
                        let said: AssistantSaid => {
                            self._hear(j, state, client_id, "assistant", said.delta)
                        },
                        let said: UserSaid => {
                            state.last_activity_ms = clock.now_ms();
                            self._hear(j, state, client_id, "user", said.delta);
                        },
                        let started: UserSpeechStarted => {
                            // It is an interruption only while audio is queued.
                            if (clock.remaining_ms() > 0) {
                                let heard = clock.stop();
                                let _ = self.audio.stop_playback() catch_all (e) {
                                    _ => 0,
                                };
                                session.interrupted(heard) catch_all (e) {
                                    _ => {}
                                };
                            }
                        },
                        let requested: ToolCalls => {
                            self._flush_transcript(j, state, client_id);
                            let pending: ai.content.ToolUse[] = [];
                            let done: ToolResult[] = [];
                            for (let call in requested.calls) {
                                self._record(
                                    j,
                                    [
                                        ai.events.ToolRequested {
                                            id: call.id,
                                            name: call.name,
                                            args: call.args,
                                        },
                                    ],
                                );
                                if (call.name == finish_tool_name()) {
                                    // The result is the run's output, so it is checked
                                    // here against `Out` and the model is told if it
                                    // does not fit.
                                    let candidate = baml.json.stringify(
                                        baml.json.to_json(call.args.get("result")) catch_all (e) {
                                            _ => null,
                                        },
                                    );
                                    let fits = true;
                                    let _ = baml.sap.parse<Out>(candidate) catch_all (e) {
                                        _ => {
                                            fits = false;
                                            null
                                        },
                                    };
                                    if (fits && !state.finished) {
                                        state.finished = true;
                                        state.final_json = candidate;
                                        state.finished_at_ms = clock.now_ms();
                                        self._record(
                                            j,
                                            [ai.events.ToolCompleted.new(call.id, "ok")],
                                        );
                                        done.push(
                                            ToolResult {
                                                id: call.id,
                                                output: "The conversation is over. Say a short goodbye if you have not yet.",
                                            },
                                        );
                                        timers.push(spawn {
                                            self._close_after(
                                                session,
                                                state,
                                                self.farewell_ms,
                                                false,
                                            )
                                        });
                                    } else {
                                        let message = `result does not match the required schema: ${candidate}`;
                                        self._record(
                                            j,
                                            [ai.events.ToolFailed { id: call.id, message: message }],
                                        );
                                        done.push(
                                            ToolResult { id: call.id, output: `error: ${message}` },
                                        );
                                    }
                                } else {
                                    pending.push(call);
                                }
                            }
                            // Tools run beside the conversation: audio keeps
                            // flowing while they work.
                            let running = spawn {
                                self._run_tools(spec.tools(), pending, done, j, session, state)
                            };
                        },
                        let reply: BackendReply => {},
                        let used: ai.events.Usage => {
                            total.input_tokens += used.input_tokens;
                            total.output_tokens += used.output_tokens;
                            if let cached: int = used.cached_input_tokens {
                                total.cached_input_tokens
                                    = (total.cached_input_tokens ?? 0) + cached;
                            }
                        },
                        let turn: TurnDone => {
                            self._flush_transcript(j, state, client_id);
                            // After the finish call, the next completed response is the farewell.
                            if (state.finished && state.spoke_since_finish) {
                                session.close();
                            }
                        },
                        let fault: SessionError => {
                            log.warn(`voice session error: ${fault.code ?? ""} ${fault.message}`);
                        },
                        let ended: Closed => {
                            if (state.close_reason == "") {
                                state.close_reason = ended.reason;
                            }
                            state.closed = true;
                        },
                    }
                }
                // A full-duplex provider has no response boundary. The farewell
                // is over once the assistant spoke and then stayed quiet.
                if (
                    state.finished
                        && state.spoke_since_finish
                        && !state.closed
                        && clock.now_ms() - state.last_speech_ms > 1500
                ) {
                    session.close();
                }
            }

            state.closed = true;
            let _ = microphone.cancel();
            for (let timer in timers) {
                let _ = timer.cancel();
            }
            // The loop also ends when the microphone pump fails to send, and
            // that path has not closed the session. Closing twice is safe.
            session.close();
            self._flush_transcript(j, state, client_id);
            self.audio.drain() catch_all (e) {
                _ => {}
            };
            self.audio.close();

            if (!state.finished) {
                if (state.timed_out) {
                    throw baml.errors.Timeout {
                        message: `the conversation went quiet or ran past ${self.max_duration_ms} ms before the model ended it`,
                        duration_ms: clock.now_ms(),
                    }
                }
                throw SessionEnded { reason: state.close_reason }
            }
            let value = baml.sap.parse<Out>(state.final_json) catch_all (e) {
                _ => {
                    throw SessionEnded { reason: `the final result no longer parses: ${state.final_json}` }
                },
            };
            self._record(j, [ai.events.FinalProduced { value_json: state.final_json }]);
            ai.RunResult { value: value, journal: j, usage: total }
        }
    }
}
