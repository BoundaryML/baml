test "guide 05.04: interruption belongs to the opened live resource" {
    let live = VoiceSupport_task("talk", root.ai.FakeRealtimeProvider { transcript: "hello" })
        .run(runner = root.ai.run.Realtime.new(root.ai.RecordingChannel { frames: [] }));
    live.cancel_response();
    live.truncate_assistant_audio(250);
    // live.cleanup();
    // live.cleanup();
    assert.equal(root.ai.collect_live_text(live), "hello");
    match (live) {
        let buffered: root.ai.BufferedLive => {
            // assert.is_true(buffered.is_closed);
            assert.equal(buffered.control_frames.length(), 2);
            assert
                .equal(
                    baml.json.path_or(baml.json.parse(buffered.control_frames[0]), ".type", ""),
                    "response.cancel",
                );
            assert
                .equal(
                    baml.json
                        .path_or(baml.json.parse(buffered.control_frames[1]), ".audio_end_ms", -1),
                    250,
                )
        },
        _ => assert.is_true(false),
    }
}

testset "integ-test-guide-05-barge-in" {
    test "guide 05.04 live OpenAI: cancel precedes played-position truncation on one session" {
        let channel = root.ai.RecordingChannel { frames: [] };
        let live = VoiceSupport_task(
            "Begin a spoken explanation of why duplicate card charges can appear.",
            root.ai.OpenAiRealtime {
                model: "gpt-realtime",
                voice: "alloy",
                api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
                output_audio: true,
            },
        )
        .run(runner = root.ai.run.Realtime.new(channel));
        // Wait only until an assistant audio item exists; the response remains live.
        let started = live.events();
        assert
            .is_true(
                started
                    .filter((event: root.ai.LiveEvent) -> bool {
                        event.kind() == "assistant_audio_started"
                    })
                    .length()
                    == 1,
            );
        live.cancel_response();
        live.truncate_assistant_audio(0);
        // Drain the provider's cancellation boundary.
        live.events();
        match (live) {
            let session: root.ai.OpenAiRealtimeLive => {
                log.info(
                    baml.json
                        .stringify(
                            {
                                "guide": "05.04",
                                "cancel_requests": session.cancel_requests,
                                "truncate_count": session.truncate_positions_ms.length(),
                                "played_ms": session.truncate_positions_ms[0],
                            },
                        ),
                );
                assert.equal(session.cancel_requests, 1);
                assert.equal(session.truncate_positions_ms, [0])
            },
            _ => assert.is_true(false),
        }
        let controls = channel.frames.filter((frame: string) -> bool {
            let kind = baml.json.path_or(baml.json.parse(frame), ".type", "");
            kind == "response.cancel" || kind == "conversation.item.truncate"
        });
        assert.equal(controls.length(), 2);
        assert
            .equal(baml.json.path_or(baml.json.parse(controls[0]), ".type", ""), "response.cancel");
        assert
            .equal(
                baml.json.path_or(baml.json.parse(controls[1]), ".type", ""),
                "conversation.item.truncate",
            )
    }
}
