test "guide 01.05: a custom runner can call a known capability directly" {
    let provider = fake_resolution();
    let task = ResolveTicket_task(sample_ticket(), provider);
    let response = task.run(
        runner = GuideGenerationCapability<Resolution> { provider: provider },
    );
    assert.equal(response.meta.provider, "fake");
    assert.equal(response.value.category, "billing")
}

testset "integ-test-guide-01-direct-provider-access" {
    test "guide 01.05 live OpenAI: a custom runner calls GenerationProvider directly" {
        let provider = live_openai();
        let response = ResolveTicket_task(sample_ticket(), provider).run(
            runner = GuideGenerationCapability<Resolution> { provider: provider },
        );
        log.info(
            "live direct capability "
                + {
                    "provider": response.meta.provider,
                    "finish_reason": response.meta.finish_reason,
                    "request_id": response.meta.request_id,
                }
                .to_string(),
        );
        assert.equal(response.meta.provider, "openai");
        assert.is_true(response.value.reply.length() > 0)
    }
}
