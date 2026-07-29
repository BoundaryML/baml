// Live public-API coverage for Anthropic structured output and SAP. Each case
// executes a typed Task through the normal completion runner.

function _anthropic_resolution_array_task(
    provider: root.ai.Provider,
) -> root.ai.Task<Resolution[]> throws never {
    root.ai.task_named<Resolution[]>(provider, "AnthropicResolutionArray", {}, [], (
        ctx: baml.llm.Context,
    ) -> baml.llm.PromptAst {
        baml.llm
            .assemble_prompt_ast(
                ["", "\n", ""],
                [
                    "Return exactly two support resolutions for a duplicate charge. Give each a non-empty reply.",
                    ctx.output_format,
                ],
            )
    })
}

testset "integ-test-anthropic-messages-configurations" {
    test "Anthropic strict completes a class-valued task" {
        let provider = root
            .anthropic
            .messages(
                api_key = baml.env.get_or_panic("ANTHROPIC_API_KEY"),
                output_mode = root.anthropic.OutputMode.Strict,
            );
        let value = ResolveTicket_task(sample_ticket(), provider)
            .run(runner = root.ai.run.Completion<Resolution>.new());
        assert.is_true(value.reply.length() > 0)
    }

    test "Anthropic strict keeps an array inside the typed result" {
        let provider = root
            .anthropic
            .messages(
                api_key = baml.env.get_or_panic("ANTHROPIC_API_KEY"),
                output_mode = root.anthropic.OutputMode.Strict,
            );
        let value = _anthropic_resolution_array_task(provider)
            .run(runner = root.ai.run.Completion<Resolution[]>.new());
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
        let value = ResolveTicket_task(sample_ticket(), provider)
            .run(runner = root.ai.run.Completion<Resolution>.new());
        assert.is_true(value.summary.length() > 0)
    }
}
