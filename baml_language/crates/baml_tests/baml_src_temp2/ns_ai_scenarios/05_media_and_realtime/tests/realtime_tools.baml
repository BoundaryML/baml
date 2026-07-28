// Tests for the realtime_tools scenario.
test "guide 05.05: realtime tasks retain the same tool roster" {
    let task = VoiceSupport_task("look up the account", root.ai.FakeRealtimeProvider { transcript: "done" });
    let with_tools = task.with_tools([lookup_account]);
    assert.equal(with_tools.tools[0].name, "lookup_account")
}

test "guide 05.05: raw realtime sessions expose tool calls without executing them" {
    let raw = root.ai.BufferedLiveSession {
        items: [
            root.ai.LiveToolCalls {
                calls: [
                    root.ai.ToolCall {
                        id: "call-raw",
                        name: "lookup_account",
                        args: { "customer_id": "C-1" },
                    },
                ],
            },
        ],
        channel: root.ai.RecordingChannel { frames: [] },
        is_closed: false,
        control_frames: [],
    };

    let events = raw.receive();
    assert.equal(events.length(), 1);
    assert.equal(events[0].kind(), "tool_calls");
    assert.equal(raw.control_frames.length(), 0)
}

test "guide 05.05: the explicit automatic-tool wrapper executes task tools" {
    let channel = root.ai.RecordingChannel { frames: [] };
    let raw = root.ai.BufferedLiveSession {
        items: [
            root.ai.LiveToolCalls {
                calls: [
                    root.ai.ToolCall {
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
    let live_session = root.ai.with_automatic_tools(raw, [lookup_account]);

    let events = live_session.receive();
    let results: root.ai.ToolResult[] = [];
    for (let event in events) {
        match (event) {
            let completed: root.ai.LiveToolResults => {
                for (let result in completed.results) {
                    results.push(result);
                }
            },
            _ => {},
        }
    }

    assert.equal(results.length(), 1);
    assert.is_true(!results[0].is_error);
    assert.equal(baml.json.path_or(results[0].output, ".status", ""), "active");
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
        let task = VoiceSupport_task(
            "Call lookup_account with customer_id C-1. Then report the returned status and tier.",
            provider,
        )
        .with_tools([lookup_account]);
        let channel = root.ai.RecordingChannel { frames: [] };
        let live_session = root.ai.with_automatic_tools(root.ai.open_live(task, channel), [lookup_account]);
        let requested = live_session.receive();
        let calls: root.ai.ToolCall[] = [];
        let results: root.ai.ToolResult[] = [];
        for (let event in requested) {
            match (event) {
                let tool_event: root.ai.LiveToolCalls => {
                    for (let call in tool_event.calls) {
                        calls.push(call);
                    }
                },
                let result_event: root.ai.LiveToolResults => {
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
        assert.is_true(!results[0].is_error);
        let transcript = root.ai.collect_live_text(live_session);
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
        assert.equal(results[0].id, calls[0].id)
    }
}
