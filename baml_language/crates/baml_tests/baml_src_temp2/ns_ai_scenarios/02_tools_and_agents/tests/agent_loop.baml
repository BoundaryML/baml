test "guide 02.02: Agent runner owns the application tool loop" {
    let provider = fake_tool_provider([one_knowledge_call()]);
    let task = ResolveTicketWithTools@task(sample_ticket()).with_provider(provider).with_tools([search_knowledge]);
    let outcome = task
        .run(
            runner = ai
                .run
                .Agent<Resolution>
                .new(
                    tools = [search_knowledge],
                    budget = ai.Budget { max_steps: 3, max_cost_usd: null },
                ),
        );
    match (outcome) {
        let done: ai.Done<Resolution> => {
            assert.equal(done.value.summary, "Resolved with tools")
        },
        _ => assert.is_true(false),
    }
}

testset "integ-test-guide-02-agent-runner" {
    test "guide 02.02 live OpenAI: Agent runner manages the complete tool loop" {
        let trace = guide_agent_trace();
        let task = ResolveTicketWithTools@task(sample_ticket()).with_provider(live_openai()).with_tools([search_knowledge]);
        let outcome = task
            .run(
                runner = ai.run.Agent<Resolution>.new(tools = [search_knowledge], budget = ai.Budget { max_steps: 4, max_cost_usd: null }, after_tool_call = (
                    event,
                ) -> {
                    trace.after_calls.push(event.call.name);
                    null
                }),
            );
        match (outcome) {
            let done: ai.Done<Resolution> => {
                log.info(
                    "live Agent runner completed "
                        + { "tool_calls": trace.after_calls, "summary": done.value.summary }
                            .to_string(),
                );
                assert.equal(trace.after_calls, ["search_knowledge"]);
                assert.is_true(done.value.reply.length() > 0)
            },
            _ => assert.is_true(false),
        }
    }
}
