test "guide 06.01: ResponseWithMetadata keeps value and provider metadata together" {
    let response = ResolveTicket_task(sample_ticket(), fake_resolution())
        .run(runner = root.ai.run.GenerationWithMeta<Resolution>.new());
    assert.equal(response.metadata.provider, "fake");
    assert.equal(response.metadata.request_id, "fake-1");
    assert.equal(response.value.category, "billing")
}

testset "integ-test-guide-06-observe-call" {
    test "guide 06.01 live OpenAI: typed value stays attached to request and usage metadata" {
        let response = ResolveTicket_task(sample_ticket(), live_openai())
            .run(runner = root.ai.run.GenerationWithMeta<Resolution>.new());
        log.info(
            baml.json
                .stringify(
                    {
                        "guide": "06.01",
                        "provider": response.metadata.provider,
                        "model": response.metadata.model,
                        "has_request_id": response.metadata.request_id != null,
                        "input_tokens": response.metadata.usage?.input_tokens ?? 0,
                        "output_tokens": response.metadata.usage?.output_tokens ?? 0,
                    },
                ),
        );
        assert.equal(response.metadata.provider, "openai");
        assert.equal(response.metadata.model, "gpt-5.6-luna");
        assert.is_true(response.metadata.request_id != null);
        assert.is_true((response.metadata.usage?.input_tokens ?? 0) > 0);
        assert.is_true((response.metadata.usage?.output_tokens ?? 0) > 0);
        assert.is_true(response.metadata.raw != null);
        assert.is_true(response.value.summary.length() > 0)
    }
}
