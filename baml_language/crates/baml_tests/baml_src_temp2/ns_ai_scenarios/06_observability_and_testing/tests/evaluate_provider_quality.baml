test "guide 06.05: provider evaluation is ordinary typed application code" {
    let value = ResolveTicket_manual(sample_ticket(), fake_resolution());
    assert.equal(resolution_score(value), 1.0)
}

testset "integ-test-guide-06-evaluate-quality" {
    test "guide 06.05 live providers: the same typed rubric scores both candidates" {
        let openai = root
            .ai
            .internal
            ._response_from_done(
                root.ai
                    .internal
                    ._require_done(
                        ResolveTicket_task(sample_ticket(), live_openai())
                            .run(runner = root.ai.run.Agent<Resolution>.new()),
                    ),
            );
        let anthropic = root
            .ai
            .internal
            ._response_from_done(
                root.ai
                    .internal
                    ._require_done(
                        ResolveTicket_task(sample_ticket(), live_anthropic())
                            .run(runner = root.ai.run.Agent<Resolution>.new()),
                    ),
            );
        let openai_score = resolution_score(openai.value);
        let anthropic_score = resolution_score(anthropic.value);
        log.info(
            baml.json
                .stringify(
                    {
                        "guide": "06.05",
                        "openai_score": openai_score,
                        "anthropic_score": anthropic_score,
                    },
                ),
        );
        assert.is_true(openai_score >= 0.5);
        assert.is_true(anthropic_score >= 0.5);
        assert.equal(openai.metadata.provider, "openai");
        assert.equal(anthropic.metadata.provider, "anthropic")
    }
}
