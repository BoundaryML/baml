// Tests for the voice library.
//
// The offline tests need no network: they cover the wire messages, the audio
// helpers, and a full VoiceAgent run against a scripted fake provider. The
// `live` testset holds real conversations on both providers with a
// synthesized caller; `baml.toml` keeps it out of a bare `baml test`.
//
//   baml test --project crates/baml_tests/baml_live
//   infisical run --env=dev -- baml test --project crates/baml_tests/baml_live --profile live

// ---------------------------------------------------------------------------
// A fake provider: replays batches of events and records what the runner sends.

class FakeSession {
    script: voice.VoiceEvent[][],
    delivered: voice.ToolResult[],
    said: string[],
    interruptions: int[],
    audio_frames: int,
    _cursor: int,
    // A batch with tool calls holds the script until their results arrive,
    // the way a model waits for its tools.
    _awaiting_results: bool,
    _closed: bool,

    function new(script: voice.VoiceEvent[][]) -> FakeSession {
        FakeSession {
            script: script,
            delivered: [],
            said: [],
            interruptions: [],
            audio_frames: 0,
            _cursor: 0,
            _awaiting_results: false,
            _closed: false,
        }
    }

    implements voice.VoiceSession {
        function send_audio(self, pcm: uint8array) -> void {
            self.audio_frames = self.audio_frames + 1;
        }

        function next_events(self) -> voice.VoiceEvent[] {
            while (self._awaiting_results && !self._closed) {
                baml.sys.sleep(baml.time.Duration.from_milliseconds(5));
            }
            if (self._closed) {
                return [voice.Closed { reason: "closed by the runner" }];
            }
            let batch: voice.VoiceEvent[] = self.script.at(self._cursor) else {
                return [voice.Closed { reason: "the script ended" }];
            };
            self._cursor = self._cursor + 1;
            for (let ev in batch) {
                if let calls: voice.ToolCalls = ev {
                    self._awaiting_results = true;
                }
            }
            batch
        }

        function send_tool_results(self, results: voice.ToolResult[]) -> void {
            for (let r in results) {
                self.delivered.push(r);
            }
            self._awaiting_results = false;
        }

        function say(self, text: string) -> void {
            self.said.push(text);
        }

        function interrupted(self, played_ms: int) -> void {
            self.interruptions.push(played_ms);
        }

        function close(self) -> void {
            self._closed = true;
        }
    }
}

class FakeProvider {
    session: FakeSession,
    configs: voice.SessionConfig[],

    implements voice.VoiceProvider {
        function id(self) -> string {
            "fake/voice"
        }

        function connect(self, config: voice.SessionConfig) -> voice.VoiceSession {
            self.configs.push(config);
            self.session
        }
    }
}

function loud_frame() -> uint8array {
    // 100 ms of a square wave at a quarter of full scale.
    let samples: int[] = [];
    let i = 0;
    while (i < 2400) {
        samples.push(0);
        samples.push(
            if (i % 48 < 24) {
                32
            } else {
                224
            },
        );
        i = i + 1;
    }
    baml.Uint8Array.from_array(samples)
}

function tool_use(id: string, name: string, args: map<string, unknown>) -> ai.content.ToolUse {
    ai.content.ToolUse { id: id, name: name, args: args }
}

class FakeRun {
    outcome: CallOutcome,
    events: string[],
    delivered: voice.ToolResult[],
    said: string[],
    tool_names: string[],
    usage: ai.events.Usage,
}

/// A whole conversation against the fake: speech, a real tool call, a finish
/// call that does not fit the schema, the corrected finish call, a farewell.
function fake_conversation() -> FakeRun {
    let script: voice.VoiceEvent[][] = [
        [voice.AssistantSaid { delta: "Thanks for " }, voice.AssistantSaid { delta: "calling." }],
        [voice.UserSaid { delta: "A table for two, " }, voice.UserSaid { delta: "under Maria." }],
        [
            ai.events.Usage {
                input_tokens: 100,
                output_tokens: 20,
                cached_input_tokens: 40,
                reasoning_tokens: null,
            },
            voice.ToolCalls {
                calls: [
                    tool_use(
                        "call_1",
                        "book_table",
                        {
                            "guest_name": "Maria",
                            "day": "tomorrow",
                            "time": "7:00 PM",
                            "party_size": 2,
                        },
                    ),
                ],
            },
            voice.TurnDone {  },
        ],
        [voice.AssistantSaid { delta: "You are booked." }, voice.TurnDone {  }],
        [voice.UserSaid { delta: "Goodbye." }],
        [voice.ToolCalls { calls: [tool_use("call_2", "finish_conversation", {})] }],
        [
            voice.ToolCalls {
                calls: [
                    tool_use(
                        "call_3",
                        "finish_conversation",
                        {
                            "result": {
                                "booked": true,
                                "confirmation_code": "LG-5287",
                                "guest_name": "Maria",
                                "party_size": 2,
                                "day": "tomorrow",
                                "time": "7:00 PM",
                                "summary": "Booked.",
                            },
                        },
                    ),
                ],
            },
        ],
        [
            voice.AssistantAudio { pcm: loud_frame(), item_id: "item_1" },
            voice.AssistantSaid { delta: "Goodbye!" },
            voice.TurnDone {  },
        ],
    ];
    let session = FakeSession.new(script);
    let provider = FakeProvider { session: session, configs: [] };
    let events: string[] = [];
    let agent = voice.VoiceAgent.new(audio = voice.ScriptedAudioDevice.new([]), provider = provider, greeting = "Greet the caller.", on_event = (
        e: ai.events.Event,
    ) -> void {
        match (e) {
            let m: ai.events.UserMessage => events.push(`user: ${m.text()}`),
            let m: ai.events.AssistantMessage => {
                for (let block in m.content) {
                    if let t: ai.content.Text = block {
                        events.push(`assistant: ${t.text}`);
                    }
                }
            },
            let r: ai.events.ToolRequested => events.push(`requested: ${r.name}`),
            let c: ai.events.ToolCompleted => events.push(`completed: ${c.id}`),
            let f: ai.events.ToolFailed => events.push(`failed: ${f.id}`),
            let f: ai.events.FinalProduced => events.push("final"),
            _ => 0,
        };
    });
    let result = agent.run(ReservationLine@spec("Luigi's"));
    let tool_names: string[] = [];
    for (let config in provider.configs) {
        for (let t in config.tools) {
            tool_names.push(t.name);
        }
    }
    FakeRun {
        outcome: result.value,
        events: events,
        delivered: session.delivered,
        said: session.said,
        tool_names: tool_names,
        usage: result.usage,
    }
}

/// A session that ends before the model finished is an error, not a value.
function fake_hangup() -> string {
    let session = FakeSession.new([[voice.UserSaid { delta: "Wrong number." }]]);
    let provider = FakeProvider { session: session, configs: [] };
    let agent = voice.VoiceAgent.new(audio = voice.ScriptedAudioDevice.new([]), provider = provider);
    let outcome = agent.run(ReservationLine@spec("Luigi's")) catch (e) {
        let ended: voice.SessionEnded => {
            // The runner releases the session on every way out of a run.
            return `ended: ${ended.reason}, session closed: ${session._closed}`;
        },
    };
    "returned a value"
}

function fake_not_a_voice_model() -> string {
    let agent = voice.VoiceAgent.new(audio = voice.ScriptedAudioDevice.new([]));
    let outcome = agent.run(TextOnly@spec("hi")) catch (e) {
        let bad: baml.errors.InvalidArgument => {
            return bad.message;
        },
    };
    "returned a value"
}

function TextOnly(q: string) -> string {
    client: "openai/gpt-4o-mini"
    prompt: `${q}`
}

// ---------------------------------------------------------------------------
// Wire messages

function live_start_message() -> baml.json.json {
    let spec = ReservationLine@spec("Luigi's");
    let tools = spec.tools().list();
    tools.push(voice.finish_tool(spec.output_type()));
    voice.OpenAILive.new().start_message(
        voice.SessionConfig { instructions: "Be brief.", tools: tools },
    )
}

function realtime_update_message() -> baml.json.json {
    let spec = ReservationLine@spec("Luigi's");
    voice.OpenAIRealtime.new(transcription_language = "en").update_message(
        voice.SessionConfig { instructions: "Be brief.", tools: spec.tools().list() },
    )
}

function clock_heard_ms() -> int[] {
    let clock = voice.PlaybackClock.new();
    // Two seconds are queued at once, the way Realtime delivers audio.
    clock.queue(96000);
    baml.sys.sleep(baml.time.Duration.from_milliseconds(300));
    let remaining = clock.remaining_ms();
    let heard = clock.stop();
    [remaining, heard, clock.remaining_ms()]
}

testset "voice" {
    test "a conversation runs a spec to its typed result" {
        let run = fake_conversation();
        assert.equal(run.outcome.booked, true);
        assert.equal(run.outcome.confirmation_code, "LG-5287");
        assert.equal(run.outcome.party_size, 2);
        // The spec's tools plus the runner's finish tool reach the provider.
        assert.equal(run.tool_names, ["check_availability", "book_table", "finish_conversation"]);
        assert.equal(run.said, ["Greet the caller."]);
        assert.equal(
            run.events,
            [
                "assistant: Thanks for calling.",
                "user: A table for two, under Maria.",
                "requested: book_table",
                "completed: call_1",
                "assistant: You are booked.",
                "user: Goodbye.",
                "requested: finish_conversation",
                "failed: call_2",
                "requested: finish_conversation",
                "completed: call_3",
                "assistant: Goodbye!",
                "final",
            ],
        );
        assert.equal(run.usage.input_tokens, 100);
        assert.equal(run.usage.cached_input_tokens, 40);
    }

    test "tool results go back to the model" {
        let run = fake_conversation();
        assert.equal(run.delivered.length(), 3);
        assert.contains(run.delivered.at(0)?.output ?? "", "LG-5287");
        // A finish call that does not fit the output type is reported, not accepted.
        assert.contains(run.delivered.at(1)?.output ?? "", "error: result does not match");
    }

    test "a session that ends early throws SessionEnded" {
        assert.equal(fake_hangup(), "ended: the script ended, session closed: true");
    }

    test "a text model is rejected before connecting" {
        assert.contains(fake_not_a_voice_model(), "is not a realtime voice model");
    }

    test "the client line selects the provider" {
        assert.equal(voice.provider_for("openai/gpt-live-1")?.id(), "openai/gpt-live-1");
        assert.equal(voice.provider_for("openai/gpt-realtime")?.id(), "openai/gpt-realtime");
        assert.equal(voice.provider_for("openai/gpt-4o-mini") == null, true);
    }

    test "GPT-Live puts the tools on the delegated backend" {
        let message = live_start_message();
        assert.equal(baml.json.path<string>(message, ".type"), "session.start");
        assert.equal(baml.json.path<string>(message, ".session.model"), "gpt-live-1");
        assert.equal(baml.json.path<int>(message, ".session.audio.format.rate"), 24000);
        assert.equal(baml.json.path<string>(message, ".session.delegation.type"), "responses");
        let tools = ".session.delegation.responses.tools";
        assert.equal(baml.json.path<string>(message, `${tools}[1].name`), "book_table");
        assert.equal(baml.json.path<string>(message, `${tools}[2].name`), "finish_conversation");
        // The finish tool's argument is the function's output type.
        assert.equal(
            baml.json.path<string>(
                message,
                `${tools}[2].parameters.properties.result.properties.booked.type`,
            ),
            "boolean",
        );
        // The voice model has no tools, so its instructions name the backend's.
        assert.contains(baml.json.path<string>(message, ".session.instructions"), "- book_table:");
    }

    test "Realtime puts the tools on the session" {
        let message = realtime_update_message();
        assert.equal(baml.json.path<string>(message, ".type"), "session.update");
        assert.equal(
            baml.json.path<string>(message, ".session.tools[0].name"),
            "check_availability",
        );
        assert.equal(
            baml.json.path<bool>(message, ".session.audio.input.turn_detection.interrupt_response"),
            true,
        );
        assert.equal(
            baml.json.path<string>(message, ".session.audio.input.transcription.language"),
            "en",
        );
    }

    test "silence is told from speech" {
        assert.equal(voice.is_silent(baml.Uint8Array.zeroes(4800)), true);
        assert.equal(voice.is_silent(loud_frame()), false);
    }

    test "a WAV header describes the PCM" {
        let file = voice.wav(loud_frame());
        assert.equal(file.length(), 44 + 4800);
        assert.equal(file.slice(0, 4).to_string(), "RIFF");
        // Sample rate 24000 = 0x5DC0, little-endian at offset 24.
        assert.equal(file.slice(24, 28).to_array(), [192, 93, 0, 0]);
        assert.equal(file.slice(40, 44).to_array(), [192, 18, 0, 0]);
    }

    test "the playback clock reports what was heard" {
        let measured = clock_heard_ms();
        let remaining = measured.at(0) ?? 0;
        let heard = measured.at(1) ?? 0;
        assert.is_true(remaining > 1400 && remaining <= 1700);
        assert.is_true(heard >= 300 && heard < 600);
        assert.equal(measured.at(2), 0);
    }
}

// ---------------------------------------------------------------------------
// Real sessions

function live_call(provider: voice.VoiceProvider) -> SelftestReport {
    scripted_call(provider, caller_script())
}

testset "live" with testing.Sequential() {
    // Always registered, so `--profile live` without a key still selects a test.
    test "preconditions" {
        assert.is_true(true)
    }

    if (baml.env.get("OPENAI_API_KEY") != null) {
        test "gpt-live books the table" {
            let report = live_call(voice.OpenAILive.new());
            assert.equal(report.outcome.booked, true);
            assert.is_true(report.tools_called.includes("book_table"));
            assert.is_true(report.code_matches_booking);
            assert.is_true(report.assistant_audio_transcript.length() > 0);
        }
        test "gpt-realtime books the table" {
            let report = live_call(voice.OpenAIRealtime.new(transcription_language = "en"));
            assert.equal(report.outcome.booked, true);
            assert.is_true(report.tools_called.includes("book_table"));
            assert.is_true(report.code_matches_booking);
            assert.is_true(report.assistant_audio_transcript.length() > 0);
        }
    } else {
        log.warn("live voice tests not registered: OPENAI_API_KEY is unset")
    }
}
