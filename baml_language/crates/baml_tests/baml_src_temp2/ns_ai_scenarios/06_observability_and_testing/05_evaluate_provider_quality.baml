function resolution_score(value: Resolution) -> float throws never {
    let score = 0.0;
    if (value.category == "billing") {
        score = score + 0.5;
    }
    if (value.reply.length() > 10) {
        score = score + 0.5;
    }
    score
}

test "guide 06.05: provider evaluation is ordinary typed application code" {
    let value = ResolveTicket_manual(sample_ticket(), fake_resolution());
    assert.equal(resolution_score(value), 1.0)
}

testset "integ-test-guide-06-evaluate-quality" {
    test "guide 06.05 live providers: the same typed rubric scores both candidates" {
        let openai = ResolveTicket_task(sample_ticket(), live_openai())
            .run(runner = root.ai.run.GenerationWithMeta<Resolution>.new());
        let anthropic = ResolveTicket_task(sample_ticket(), live_anthropic())
            .run(runner = root.ai.run.GenerationWithMeta<Resolution>.new());
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
        assert.equal(openai.meta.provider, "openai");
        assert.equal(anthropic.meta.provider, "anthropic")
    }
}
