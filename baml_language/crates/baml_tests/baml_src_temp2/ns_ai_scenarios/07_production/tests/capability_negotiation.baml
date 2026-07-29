test "guide 07.04: erased providers can be negotiated explicitly" {
    let providers: root.ai.Provider[] = [fake_resolution(), root.ai.testing.FakeRealtimeProvider { transcript: "hello" }];
    let capabilities = providers.map((provider: root.ai.Provider) -> string {
        match (provider) {
            let realtime: root.ai.realtime.RealtimeProvider => "realtime",
            let agent: root.ai.AgentProvider => "agent",
            _ => "provider-only",
        }
    });
    assert.equal(capabilities, ["agent", "realtime"]);
    let runs = providers.map((provider: root.ai.Provider) -> NegotiatedRun {
        ResolveTicket_task(sample_ticket(), provider)
            .run(runner = CapabilityNegotiationRunner.new())
    });
    assert
        .equal(
            runs.map((run: NegotiatedRun) -> string {
                run.mode
            }),
            ["agent", "realtime"],
        )
}

testset "integ-test-guide-07-capability-negotiation" {
    test "guide 07.04 live providers: erased configuration selects stream and realtime paths" {
        let providers: root.ai.Provider[] = [
            live_openai(),
            root.openai.Realtime {
                model: "gpt-realtime",
                voice: "alloy",
                api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
                output_audio: false,
                input_transcription_model: null,
                server_vad: null,
                keep_open: false,
            },
        ];
        //# Run each erased provider only after its capability match succeeds
        let runs = providers.map((provider: root.ai.Provider) -> NegotiatedRun {
            ResolveTicket_task(sample_ticket(), provider)
                .run(runner = CapabilityNegotiationRunner.new())
        });
        log.info(
            baml.json
                .stringify(
                    {
                        "guide": "07.04",
                        "providers": runs
                            .map((run: NegotiatedRun) -> string {
                                run.provider
                            })
                            .join(","),
                        "modes": runs
                            .map((run: NegotiatedRun) -> string {
                                run.mode
                            })
                            .join(","),
                        "output_lengths": runs
                            .map((run: NegotiatedRun) -> string {
                                run.output.length().to_string()
                            })
                            .join(","),
                    },
                ),
        );
        assert
            .equal(
                runs.map((run: NegotiatedRun) -> string {
                    run.mode
                }),
                ["stream", "realtime"],
            );
        assert.is_true(runs[0].output.length() > 0);
        assert.is_true(runs[1].output.length() > 0)
    }
}
