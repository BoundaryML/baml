test "guide 05.06: call-center scheduling tool exposes every required field" {
    let tool = root.ai.normalize_tool(root.ai_scenarios.schedule_meeting);
    assert.equal(tool.name, "schedule_meeting");
    let required = baml.json.path_or<json[]>(tool.input_schema, ".required", []);
    assert.equal(required.length(), 7)
}

test "guide 05.06: weather is an explicit fake task tool" {
    let tool = root.ai.normalize_tool(root.ai_scenarios.check_weather);
    assert.equal(tool.name, "check_weather");
    let required = baml.json.path_or<string[]>(tool.input_schema, ".required", []);
    assert.equal(required, ["location", "date"]);

    let report = root.ai_scenarios.check_weather("San Francisco", "2026-07-31");
    assert.equal(report.location, "San Francisco");
    assert.equal(report.date, "2026-07-31");
    assert.equal(report.conditions, "Sunny with a light breeze");
    assert.equal(report.temperature_fahrenheit, 72);
    assert.equal(report.precipitation_chance_percent, 5)
}

test "guide 05.06: conversation completion is an explicit task tool" {
    let tool = root.ai.normalize_tool(root.ai_scenarios.mark_conversation_complete);
    assert.equal(tool.name, "mark_conversation_complete");
    let required = baml.json.path_or<string[]>(tool.input_schema, ".required", []);
    assert.equal(required, ["reason"]);

    let task = root
        .ai_scenarios
        .realtime_call_center_task(
            root.ai.FakeRealtimeProvider { transcript: "hello" },
            "Say hello.",
        );
    assert
        .is_true(
            task.tools.some((candidate: root.ai.Tool) -> bool {
                candidate.name == "schedule_meeting"
            }),
        );
    assert
        .is_true(
            task.tools.some((candidate: root.ai.Tool) -> bool {
                candidate.name == "check_weather"
            }),
        );
    assert
        .is_true(
            task.tools.some((candidate: root.ai.Tool) -> bool {
                candidate.name == "mark_conversation_complete"
            }),
        )
}

test "guide 05.06: a duplex voice agent pumps microphone input beside session events" {
    let audio = root.ai.BufferedRealtimeAudioDevice {
        input_audio_format: root.ai.fake_realtime_audio_format(),
        output_audio_format: root.ai.fake_realtime_audio_format(),
        input_chunks: [],
        next_input_index: 0,
        output_chunks: [],
        played_ms: 0,
        is_closed: false,
    };
    let task = root
        .ai_scenarios
        .realtime_call_center_task(
            root.ai.FakeRealtimeProvider { transcript: "hello" },
            "Say hello.",
        );
    task.run(runner = root.ai.run.VoiceAgent.new(audio, root.ai.RecordingChannel { frames: [] }));
    assert.is_true(audio.is_closed)
}

test "guide 05.06: voice agent rejects an incompatible audio device" {
    let audio = root.ai.BufferedRealtimeAudioDevice {
        input_audio_format: root.ai.RealtimeAudioFormat { encoding: "other", sample_rate_hz: 8_000, channels: 1 },
        output_audio_format: root.ai.fake_realtime_audio_format(),
        input_chunks: [],
        next_input_index: 0,
        output_chunks: [],
        played_ms: 0,
        is_closed: false,
    };
    let task = root
        .ai_scenarios
        .realtime_call_center_task(
            root.ai.FakeRealtimeProvider { transcript: "hello" },
            "Say hello.",
        );
    let rejected = false;
    task.run(runner = root.ai.run.VoiceAgent.new(audio, root.ai.RecordingChannel { frames: [] })) catch (e) {
        let unsupported: baml.errors.Unsupported => {
            rejected = unsupported.message.includes("input format mismatch");
            null
        },
    };
    assert.is_true(rejected);
    assert.is_true(audio.is_closed)
}

class LatestWinsRealtimeProvider {
    events: root.ai.LiveEvent[],

    implements root.ai.Provider {
        function name(self) -> string throws never {
            "latest-wins-realtime"
        }
        function render_shorthand(self) -> string throws never {
            "fake/latest-wins"
        }
    }
    implements root.ai.RealtimeProvider {
        function open_live<T>(
            self,
            task: root.ai.Task<T>,
            channel: root.ai.Channel,
        ) -> root.ai.LiveSession throws never {
            root.ai.BufferedLiveSession {
                items: self.events,
                channel: channel,
                is_closed: false,
                control_frames: [],
            }
        }
    }
}

test "guide 05.06: a newer assistant item replaces queued older audio" {
    let old_audio = baml.Uint8Array.from_base64("AQI=");
    let latest_audio = baml.Uint8Array.from_base64("AwQ=");
    let audio = root.ai.BufferedRealtimeAudioDevice {
        input_audio_format: root.ai.fake_realtime_audio_format(),
        output_audio_format: root.ai.fake_realtime_audio_format(),
        input_chunks: [],
        next_input_index: 0,
        output_chunks: [],
        played_ms: 0,
        is_closed: false,
    };
    let provider = LatestWinsRealtimeProvider {
        events: [
            root.ai.AssistantAudioStarted { item_id: "old" },
            root.ai.AssistantAudioDelta { audio: old_audio },
            root.ai.LiveTurnCompleted {  },
            root.ai.AssistantAudioStarted { item_id: "latest" },
            root.ai.AssistantAudioDelta { audio: latest_audio },
            root.ai.LiveClosed {  },
        ],
    };
    let task = root.ai_scenarios.realtime_call_center_task(provider, "Say two things.");

    task.run(runner = root.ai.run.VoiceAgent.new(audio, root.ai.RecordingChannel { frames: [] }));

    assert.equal(audio.output_chunks, [latest_audio])
}

test "guide 05.06: sustained server-VAD speech stops playback, cancels, then truncates" {
    let assistant_audio = baml.Uint8Array.from_base64("AQI=");
    let audio = root.ai.BufferedRealtimeAudioDevice {
        input_audio_format: root.ai.fake_realtime_audio_format(),
        output_audio_format: root.ai.fake_realtime_audio_format(),
        input_chunks: [],
        next_input_index: 0,
        output_chunks: [assistant_audio],
        played_ms: 625,
        is_closed: false,
    };
    let channel = root.ai.RecordingChannel { frames: [] };
    let live_session = root.ai.BufferedLiveSession {
        items: [],
        channel: channel,
        is_closed: false,
        control_frames: [],
    };
    let state = root.ai.run.VoiceAgentState {
        assistant_audio_active: true,
        assistant_response_active: true,
        user_speaking: true,
        user_speech_item_id: "user-1",
        user_turn_pending: false,
        speech_generation: 1,
        closed: false,
    };

    root.ai.run.interrupt_after_sustained_user_speech(state, 1, 0, audio, live_session);

    assert.equal(audio.output_chunks.length(), 0);
    assert.is_true(!state.assistant_audio_active);
    assert.equal(live_session.control_frames.length(), 2);
    assert
        .equal(
            baml.json.path_or(baml.json.parse(live_session.control_frames[0]), ".type", ""),
            "response.cancel",
        );
    assert
        .equal(
            baml.json.path_or(baml.json.parse(live_session.control_frames[1]), ".audio_end_ms", -1),
            625,
        )
}

test "guide 05.06: truncation cannot exceed the exact assistant item's audio" {
    assert.equal(root.openai.realtime_audio_end_ms(9475, 8950 * 48), 8950);
    assert.equal(root.openai.realtime_audio_end_ms(4000, 8950 * 48), 4000);
    assert.equal(root.openai.realtime_audio_end_ms(-1, 8950 * 48), 0)
}

test "guide 05.06: speech stopped before the threshold does not interrupt" {
    let assistant_audio = baml.Uint8Array.from_base64("AQI=");
    let audio = root.ai.BufferedRealtimeAudioDevice {
        input_audio_format: root.ai.fake_realtime_audio_format(),
        output_audio_format: root.ai.fake_realtime_audio_format(),
        input_chunks: [],
        next_input_index: 0,
        output_chunks: [assistant_audio],
        played_ms: 625,
        is_closed: false,
    };
    let live_session = root.ai.BufferedLiveSession {
        items: [],
        channel: root.ai.RecordingChannel { frames: [] },
        is_closed: false,
        control_frames: [],
    };
    let state = root.ai.run.VoiceAgentState {
        assistant_audio_active: true,
        assistant_response_active: true,
        user_speaking: false,
        user_speech_item_id: null,
        user_turn_pending: false,
        speech_generation: 1,
        closed: false,
    };

    root.ai.run.interrupt_after_sustained_user_speech(state, 1, 0, audio, live_session);

    assert.equal(audio.output_chunks, [assistant_audio]);
    assert.is_true(state.assistant_audio_active);
    assert.equal(live_session.control_frames.length(), 0)
}

test "guide 05.06: server VAD is realtime session configuration" {
    let update = baml
        .json
        .parse(
            root.openai
                .openai_realtime_session_update(
                    "schedule meetings",
                    "marin",
                    "gpt-4o-mini-transcribe",
                    root.openai.RealtimeServerVad {
                        threshold: 0.5,
                        prefix_padding_ms: 300,
                        silence_duration_ms: 500,
                        create_response: false,
                        interrupt_response: false,
                    },
                ),
        );
    assert
        .equal(
            baml.json.path_or(update, ".session.audio.input.turn_detection.type", ""),
            "server_vad",
        );
    assert
        .equal(
            baml.json.path_or(update, ".session.audio.input.turn_detection.create_response", true),
            false,
        );
    assert
        .equal(
            baml.json
                .path_or(update, ".session.audio.input.turn_detection.interrupt_response", true),
            false,
        )
}

testset "integ-test-guide-05-realtime-call-center" {
    test "guide 05.06 live OpenAI: the agent waits for complete confirmed meeting details" {
        let provider = root.openai.Realtime {
            model: "gpt-realtime-2.1",
            voice: "marin",
            api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
            output_audio: false,
            instructions: root.ai_scenarios.realtime_call_center_instructions(),
            input_transcription_model: null,
            server_vad: null,
            keep_open: true,
        };
        let task = root
            .ai_scenarios
            .realtime_call_center_task(
                provider,
                "Introduce yourself in one short sentence and ask how you can help.",
            );
        let live_session = root
            .ai
            .with_automatic_tools(
                root.ai.open_live(task, root.ai.RecordingChannel { frames: [] }),
                root.ai.tool_inputs_from_tools(task.tools),
            );

        let opening = root.ai.collect_live_turn(live_session);
        assert.equal(opening.tool_calls.length(), 0);

        live_session.send_text("My name is Aaron, and I want a meeting about BAML.");
        let incomplete = root.ai.collect_live_turn(live_session);
        assert.equal(incomplete.tool_calls.length(), 0);

        live_session
            .send_text(
                "Use aaron@example.com. Book July 31, 2026 at 2:00 PM "
                    + "in America/Los_Angeles for 30 minutes to discuss BAML. "
                    + "Those details are correct and I explicitly confirm them.",
            );
        let complete = root.ai.collect_live_turn(live_session);
        assert.equal(complete.tool_calls.length(), 1);
        assert.equal(complete.tool_calls[0].name, "schedule_meeting");
        assert
            .equal(
                baml.json.path_or(complete.tool_calls[0].args, ".attendee_email", ""),
                "aaron@example.com",
            );
        assert
            .equal(
                baml.json.path_or(complete.tool_calls[0].args, ".timezone", ""),
                "America/Los_Angeles",
            );
        assert.equal(complete.tool_results.length(), 1);
        assert.is_true(!complete.tool_results[0].is_error);
        log.info(
            {
                "guide": "05.06",
                "premature_tool_calls": incomplete.tool_calls.length(),
                "tool": complete.tool_calls[0],
                "tool_result": complete.tool_results[0],
                "confirmation": complete.assistant_transcript,
            },
        );
        assert.is_true(complete.assistant_transcript.length() > 0);

        live_session.send_text("Thanks, that is everything I needed. Goodbye.");
        let ending = root.ai.collect_live_turn(live_session);
        assert
            .is_true(
                ending.tool_calls.some((call: root.ai.ToolCall) -> bool {
                    call.name == "mark_conversation_complete"
                }),
            );
        assert
            .is_true(
                ending.tool_results.some((result: root.ai.ToolResult) -> bool {
                    !result.is_error
                }),
            );
        live_session.close()
    }
}
