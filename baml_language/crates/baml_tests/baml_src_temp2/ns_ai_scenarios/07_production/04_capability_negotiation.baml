test "guide 07.04: erased providers can be negotiated explicitly" {
    let providers: root.ai.Provider[] = [fake_resolution(), root.ai.FakeRealtimeProvider { transcript: "hello" }];
    let capabilities = providers.map((provider: root.ai.Provider) -> string {
        match (provider) {
            let realtime: root.ai.RealtimeProvider => "realtime",
            let completion: root.ai.CompletionProvider => "completion",
            _ => "provider-only",
        }
    });
    assert.equal(capabilities, ["completion", "realtime"])
}

class NegotiatedRun {
    provider: string,
    mode: string,
    output: string,
}

function run_negotiated(
    provider: root.ai.Provider,
) -> NegotiatedRun throws root.ai.CallError
    | baml.errors.UnknownError
    | root.ai.StreamError
    | baml.errors.Unsupported
    | baml.errors.Io
    | baml.errors.LlmClient
{
    //# Narrow the erased provider to the strongest interaction it actually supports
    match (provider) {
        let realtime: root.ai.RealtimeProvider => {
            let channel = root.ai.RecordingChannel { frames: [] };
            // Calling the concrete runner directly avoids the current compiler's
            // unresolved associated-Error projection in an explicitly-throws helper.
            let runner = root.ai.run.Realtime.new(channel);
            let live = runner.run(VoiceSupport_task("Reply with exactly: negotiated realtime", realtime));
            NegotiatedRun {
                provider: realtime.name(),
                mode: "realtime",
                output: root.ai.collect_live_text(live),
            }
        },
        let stream: root.ai.StreamingProvider => {
            let resolution = drain_resolution_stream(ResolveTicket_task(sample_ticket(), stream));
            NegotiatedRun { provider: stream.name(), mode: "stream", output: resolution.reply }
        },
        let completion: root.ai.CompletionProvider => {
            // Concrete runner dispatch preserves the associated Error through this
            // explicitly-annotated helper on the current baml-cli compiler path.
            let runner = root.ai.run.Completion<Resolution>.new();
            let resolution = runner.run(ResolveTicket_task(sample_ticket(), completion));
            NegotiatedRun {
                provider: completion.name(),
                mode: "completion",
                output: resolution.reply,
            }
        },
        _ => {
            throw baml.errors.Unsupported {
      message: "configured provider cannot resolve a ticket",
    }
        },
    }
}

testset "integ-test-guide-07-capability-negotiation" {
    test "guide 07.04 live providers: erased configuration selects stream and realtime paths" {
        let providers: root.ai.Provider[] = [
            live_openai(),
            root.ai.OpenAiRealtime {
                model: "gpt-realtime",
                voice: "alloy",
                api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
                output_audio: false,
            },
        ];
        //# Run each erased provider only after its capability match succeeds
        let runs = providers.map((provider: root.ai.Provider) -> NegotiatedRun {
            run_negotiated(provider)
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
        assert.contains(runs[1].output.to_lower_case(), "negotiated realtime")
    }
}
