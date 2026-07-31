// Live public-API coverage for Anthropic structured output and SAP. Each case
// executes a typed Task through the normal Agent runner.

function AnthropicResolutionArray() -> Resolution[] {
    provider: ai.testing.fake_output_provider("[]")
    prompt: `
        Return exactly two support resolutions for a duplicate charge. Give
        each a non-empty reply.
        ${ctx.output_format}
    `
}

testset "integ-test-anthropic-messages-configurations" {
    test "Anthropic strict completes a class-valued task" {
        let provider = root
            .anthropic
            .messages(
                api_key = baml.env.get_or_panic("ANTHROPIC_API_KEY"),
                output_mode = root.anthropic.OutputMode.Strict,
            );
        let value = done_or_fail(
                ResolveTicket@task(sample_ticket()).with_provider(provider)
                    .run(runner = ai.run.Agent<Resolution>.new()),
            )
            .value;
        assert.is_true(value.reply.length() > 0)
    }

    test "Anthropic strict keeps an array inside the typed result" {
        let provider = root
            .anthropic
            .messages(
                api_key = baml.env.get_or_panic("ANTHROPIC_API_KEY"),
                output_mode = root.anthropic.OutputMode.Strict,
            );
        let value = done_or_fail(
                AnthropicResolutionArray@task().with_provider(provider)
                    .run(runner = ai.run.Agent<Resolution[]>.new()),
            )
            .value;
        assert.equal(value.length(), 2);
        assert.is_true(value[0].reply.length() > 0);
        assert.is_true(value[1].reply.length() > 0)
    }

    test "Anthropic SAP completes a class-valued task" {
        let provider = root
            .anthropic
            .messages(
                api_key = baml.env.get_or_panic("ANTHROPIC_API_KEY"),
                output_mode = root.anthropic.OutputMode.Sap,
            );
        let value = done_or_fail(
                ResolveTicket@task(sample_ticket()).with_provider(provider)
                    .run(runner = ai.run.Agent<Resolution>.new()),
            )
            .value;
        assert.is_true(value.summary.length() > 0)
    }

    test "Anthropic strict streams a class-valued task" {
        let provider = root
            .anthropic
            .messages(
                api_key = baml.env.get_or_panic("ANTHROPIC_API_KEY"),
                output_mode = root.anthropic.OutputMode.Strict,
            );
        let value = drain_resolution_stream(ResolveTicket@task(sample_ticket()).with_provider(provider));
        assert.is_true(value.reply.length() > 0)
    }
}
