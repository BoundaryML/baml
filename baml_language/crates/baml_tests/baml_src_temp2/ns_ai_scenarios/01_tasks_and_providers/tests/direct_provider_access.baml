test "guide 01.05: a custom runner can call a known capability directly" {
    let provider = fake_resolution();
    let task = ResolveTicket_task(sample_ticket(), provider);
    let response = task.run(runner = GuideAgentCapability<Resolution> { provider: provider });
    assert.equal(response.metadata.provider, "fake");
    assert.equal(response.value.category, "billing")
}

testset "integ-test-guide-01-direct-provider-access" {
    test "guide 01.05 live OpenAI: a custom runner pins an AgentProvider" {
        let provider = live_openai();
        let response = ResolveTicket_task(sample_ticket(), provider)
            .run(runner = GuideAgentCapability<Resolution> { provider: provider });
        log.info(
            "live direct capability "
                + {
                    "provider": response.metadata.provider,
                    "finish_reason": response.metadata.finish_reason,
                    "request_id": response.metadata.request_id,
                }
                .to_string(),
        );
        assert.equal(response.metadata.provider, "openai");
        assert.is_true(response.value.reply.length() > 0)
    }
}
