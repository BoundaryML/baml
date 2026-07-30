test "guide 06.03: usage is aggregatable without becoming continuation state" {
    let first = ai
        .internal
        ._response_from_done(
            ai
                .internal
                ._require_done(
                    ResolveTicket@task(sample_ticket()).with_provider(fake_resolution())
                        .run(runner = ai.run.Agent<Resolution>.new()),
                ),
        );
    let second = ai
        .internal
        ._response_from_done(
            ai
                .internal
                ._require_done(
                    ResolveTicket@task(sample_ticket()).with_provider(fake_resolution())
                        .run(runner = ai.run.Agent<Resolution>.new()),
                ),
        );
    let usage = ai.observe.aggregate_usage([first.metadata, second.metadata]);
    assert.equal(usage.input_tokens, 2);
    assert
        .equal(
            ai
                .observe
                .estimated_cost(
                    usage,
                    ai.observe.TokenPrice { input_per_million: 1.0, output_per_million: 2.0 },
                ),
            0.000006,
        )
}

testset "integ-test-guide-06-usage-and-cost" {
    test "guide 06.03 live providers: aggregate exact counters and estimate explicit price" {
        let first = ai
            .internal
            ._response_from_done(
                ai
                    .internal
                    ._require_done(
                        ResolveTicket@task(sample_ticket()).with_provider(live_openai())
                            .run(runner = ai.run.Agent<Resolution>.new()),
                    ),
            );
        let second = ai
            .internal
            ._response_from_done(
                ai
                    .internal
                    ._require_done(
                        ResolveTicket@task(sample_ticket()).with_provider(live_anthropic())
                            .run(runner = ai.run.Agent<Resolution>.new()),
                    ),
            );
        let first_usage = first.metadata.usage
            ?? ai.Usage {
                input_tokens: 0,
                output_tokens: 0,
                cached_input_tokens: 0,
                reasoning_tokens: 0,
                cost_usd: null,
            };
        let second_usage = second.metadata.usage
            ?? ai.Usage {
                input_tokens: 0,
                output_tokens: 0,
                cached_input_tokens: 0,
                reasoning_tokens: 0,
                cost_usd: null,
            };
        let aggregate = ai.observe.aggregate_usage([first.metadata, second.metadata]);
        let estimate = ai
            .observe
            .estimated_cost(
                aggregate,
                ai.observe.TokenPrice { input_per_million: 1.0, output_per_million: 2.0 },
            );
        log.info(
            baml.json
                .stringify(
                    {
                        "guide": "06.03",
                        "responses": 2,
                        "input_tokens": aggregate.input_tokens,
                        "output_tokens": aggregate.output_tokens,
                        "estimated_cost": estimate,
                    },
                ),
        );
        assert.equal(aggregate.input_tokens, first_usage.input_tokens + second_usage.input_tokens);
        assert
            .equal(aggregate.output_tokens, first_usage.output_tokens + second_usage.output_tokens);
        assert.is_true(aggregate.input_tokens > 0);
        assert.is_true(aggregate.output_tokens > 0);
        assert.is_true(estimate > 0.0)
    }
}
