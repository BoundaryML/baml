// Live public-API coverage for Anthropic-native strict function calling. The
// Agent runner executes the application tool and continues to a typed result.

testset "integ-test-anthropic-native-tools" {
    test "Anthropic strict calls an application tool before returning the typed result" {
        let provider = root
            .anthropic
            .messages(
                api_key = baml.env.get_or_panic("ANTHROPIC_API_KEY"),
                output_mode = root.anthropic.OutputMode.Strict,
                tool_mode = root.anthropic.ToolMode.Native,
            );
        let task = ResolveTicketWithTools_task(sample_ticket(), provider, [search_knowledge]);
        let outcome = task.run(runner = root.ai.run.Agent<Resolution>.new(tools = [search_knowledge])) catch_all (e) {
            _ => {
                log.error({ "anthropic_native_tool_failure": e });
                throw baml.errors.UnknownError.with_message<never>(
                    e,
                    "Anthropic native tool integration failed",
                )
            },
        };
        match (outcome) {
            let done: root.ai.Done<Resolution> => assert.is_true(done.value.reply.length() > 0),
            _ => assert.is_true(false),
        }
    }
}
