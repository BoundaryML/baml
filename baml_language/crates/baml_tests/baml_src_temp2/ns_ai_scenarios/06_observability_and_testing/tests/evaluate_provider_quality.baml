test "guide 06.05: provider evaluation is ordinary typed application code" {
    let value = ai.internal._require_done(ResolveTicket@task(sample_ticket()).with_provider(fake_resolution()).run(runner = ai.run.Agent<Resolution>.new())).value;
    assert.equal(resolution_score(value), 1.0)
}

testset "integ-test-guide-06-evaluate-quality" {
    test "guide 06.05 live providers: the same typed rubric scores both candidates" {
        let openai = ai
            .internal
            ._response_from_done(
                ai
                    .internal
                    ._require_done(
                        ResolveTicket@task(sample_ticket()).with_provider(live_openai())
                            .run(runner = ai.run.Agent<Resolution>.new()),
                    ),
            );
        let anthropic = ai
            .internal
            ._response_from_done(
                ai
                    .internal
                    ._require_done(
                        ResolveTicket@task(sample_ticket()).with_provider(live_anthropic())
                            .run(runner = ai.run.Agent<Resolution>.new()),
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
