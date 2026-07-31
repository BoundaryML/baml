// Tests for the realtime_tools scenario.
test "guide 05.05: realtime tasks retain the same tool roster" {
    let task = VoiceSupport@task("look up the account").with_provider(ai.testing.FakeRealtimeProvider { transcript: "done" });
    let with_tools = task.with_tools([lookup_account]);
    assert.equal(with_tools.tools[0].name, "lookup_account")
}

test "guide 05.05: raw realtime sessions expose tool calls without executing them" {
    let raw = ai.testing.BufferedLiveSession {
        items: [
            ai.realtime.LiveToolCalls {
                calls: [
                    ai.tools.ToolCall {
                        id: "call-raw",
                        name: "lookup_account",
                        args: { "customer_id": "C-1" },
                    },
                ],
            },
        ],
        channel: ai.testing.RecordingChannel { frames: [] },
        is_closed: false,
        control_frames: [],
    };

    let events = raw.receive();
    assert.equal(events.length(), 1);
    assert.equal(events[0].kind(), "tool_calls");
    assert.equal(raw.control_frames.length(), 0)
}

test "guide 05.05: the explicit automatic-tool wrapper executes task tools" {
    let channel = ai.testing.RecordingChannel { frames: [] };
    let raw = ai.testing.BufferedLiveSession {
        items: [
            ai.realtime.LiveToolCalls {
                calls: [
                    ai.tools.ToolCall {
                        id: "call-1",
                        name: "lookup_account",
                        args: { "customer_id": "C-1" },
                    },
                ],
            },
        ],
        channel: channel,
        is_closed: false,
        control_frames: [],
    };
    let live_session = ai.realtime.with_automatic_tools(raw, [lookup_account]);

    let events = live_session.receive();
    let results: ai.tools.ToolResult[] = [];
    for (let event in events) {
        match (event) {
            let completed: ai.realtime.LiveToolResults => {
                for (let result in completed.results) {
                    results.push(result);
                }
            },
            _ => {},
        }
    }

    assert.equal(results.length(), 1);
    assert.is_true(!ai.tools.result_is_error(results[0]));
    assert.equal(baml.json.path_or(ai.tools.result_payload(results[0]), ".status", ""), "active");
    assert.equal(raw.control_frames.length(), 1)
}

testset "integ-test-guide-05-realtime-tools" {
    test "guide 05.05 live OpenAI: explicit automatic tools splice a correlated result into the session" {
        let provider = root.openai.Realtime {
            model: "gpt-realtime",
            voice: "alloy",
            api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
            output_audio: false,
            input_transcription_model: null,
            server_vad: null,
            keep_open: false,
        };
        let task = VoiceSupport@task("Call lookup_account with customer_id C-1. Then report the returned status and tier.").with_provider(provider)
        .with_tools([lookup_account]);
        let channel = ai.testing.RecordingChannel { frames: [] };
        let live_session = ai
            .realtime
            .with_automatic_tools(ai.realtime.open_live(task, channel), [lookup_account]);
        let requested = live_session.receive();
        let calls: ai.tools.ToolCall[] = [];
        let results: ai.tools.ToolResult[] = [];
        for (let event in requested) {
            match (event) {
                let tool_event: ai.realtime.LiveToolCalls => {
                    for (let call in tool_event.calls) {
                        calls.push(call);
                    }
                },
                let result_event: ai.realtime.LiveToolResults => {
                    for (let result in result_event.results) {
                        results.push(result);
                    }
                },
                _ => {},
            }
        }
        assert.equal(calls.length(), 1);
        assert.equal(calls[0].name, "lookup_account");
        assert.equal(baml.json.path_or(calls[0].args, ".customer_id", ""), "C-1");
        assert.equal(results.length(), 1);
        assert.is_true(!ai.tools.result_is_error(results[0]));
        let transcript = ai.realtime.collect_live_text(live_session);
        log.info(
            baml.json
                .stringify(
                    {
                        "guide": "05.05",
                        "tool_name": calls[0].name,
                        "tool_call_id": calls[0].id,
                        "submitted_results": results.length(),
                        "transcript_length": transcript.length(),
                    },
                ),
        );
        assert.contains(transcript.to_lower_case(), "active");
        assert.contains(transcript.to_lower_case(), "pro");
        assert.equal(ai.tools.result_id(results[0]), calls[0].id)
    }
}
