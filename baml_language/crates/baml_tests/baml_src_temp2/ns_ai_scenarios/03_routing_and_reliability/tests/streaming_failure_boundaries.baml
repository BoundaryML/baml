// Tests for the streaming_failure_boundaries scenario.
test "guide 03.07: an Agent retry wrapper does not invent stream replay" {
    let provider: ai.Provider = ai.retry(fake_resolution(), 2);
    let capability = match (provider) {
        let streaming: ai.StreamingProvider => "streaming",
        _ => "not-streaming",
    };
    assert.equal(capability, "not-streaming")
}

testset "integ-test-guide-03-streaming-failure-boundary" {
    test "guide 03.07 live OpenAI: streaming works directly but retry does not claim it" {
        let provider = live_openai();
        let value = drain_resolution_stream(ResolveTicket@task(sample_ticket()).with_provider(provider));
        let wrapped: ai.Provider = ai.retry(provider, 2);
        let capability = match (wrapped) {
            let streaming: ai.StreamingProvider => "streaming",
            _ => "not-streaming",
        };
        log.info(
            "live stream replay boundary "
                + { "direct_stream_reply": value.reply, "wrapped_capability": capability }
                    .to_string(),
        );
        assert.is_true(value.reply.length() > 0);
        assert.equal(capability, "not-streaming")
    }
}
