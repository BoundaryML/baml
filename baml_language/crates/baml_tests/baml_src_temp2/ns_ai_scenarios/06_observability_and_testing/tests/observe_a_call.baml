test "guide 06.01: ResponseWithMetadata keeps value and provider metadata together" {
    let response = ResolveTicket_task(sample_ticket(), fake_resolution())
        .run(runner = root.ai.run.GenerationWithMeta<Resolution>.new());
    assert.equal(response.meta.provider, "fake");
    assert.equal(response.meta.request_id, "fake-1");
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
                        "provider": response.meta.provider,
                        "model": response.meta.model,
                        "has_request_id": response.meta.request_id != null,
                        "input_tokens": response.meta.usage?.input_tokens ?? 0,
                        "output_tokens": response.meta.usage?.output_tokens ?? 0,
                    },
                ),
        );
        assert.equal(response.meta.provider, "openai");
        assert.equal(response.meta.model, "gpt-5.6-luna");
        assert.is_true(response.meta.request_id != null);
        assert.is_true((response.meta.usage?.input_tokens ?? 0) > 0);
        assert.is_true((response.meta.usage?.output_tokens ?? 0) > 0);
        assert.is_true(response.meta.raw != null);
        assert.is_true(response.value.summary.length() > 0)
    }
}
