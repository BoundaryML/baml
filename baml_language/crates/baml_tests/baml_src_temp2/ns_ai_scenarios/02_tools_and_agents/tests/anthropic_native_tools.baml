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
        let trace = guide_agent_trace();
        let task = ToolInstructions@task(`Call search_knowledge exactly once with query "duplicate charge". After its result, return a final resolution with a non-empty summary and reply.`).with_provider(provider).with_tools([search_knowledge]);
        let outcome = task
            .run(
                runner = ai.run.Agent<Resolution>.new(tools = [search_knowledge], after_tool_call = (event) -> {
                    trace.after_calls.push(event.call.name);
                    null
                }),
            ) catch_all (e) {
            _ => {
                log.error({ "anthropic_native_tool_failure": e });
                throw baml.errors.UnknownError.with_message<never>(
                    e,
                    "Anthropic native tool integration failed",
                )
            },
        };
        assert.equal(trace.after_calls, ["search_knowledge"]);
        match (outcome) {
            let done: ai.Done<Resolution> => {
                assert.is_true(done.value.summary.length() > 0);
                assert.is_true(done.value.reply.length() > 0)
            },
            _ => assert.is_true(false),
        }
    }
}
