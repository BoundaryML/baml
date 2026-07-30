// Let the model keep working across tool calls until it has enough information
// to return a complete support resolution.
function agent_loop() -> null {
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
    log.info(outcome);
    null
}
