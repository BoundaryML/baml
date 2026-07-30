// Return control to the application when work needs another specialist, and
// stop agent work cleanly when its step budget is exhausted.
function handoffs_and_budgets() -> null {
    let handoff_tool = ai.tools.tool(lookup_account).as_handoff();
    let handoff_task = ResolveTicketWithTools@task(sample_ticket()).with_provider(fake_tool_provider(
            [
                ai.tools.ToolCall {
                    id: "handoff-1",
                    name: "lookup_account",
                    args: { "customer_id": "C-1" },
                },
            ],
        )).with_tools([handoff_tool]);
    let handoff = handoff_task.run(runner = ai.run.Agent<Resolution>.new(tools = [handoff_tool]));

    let budget_task = ResolveTicketWithTools@task(sample_ticket()).with_provider(fake_tool_provider([one_knowledge_call()])).with_tools([search_knowledge]);
    let budget = budget_task
        .run(
            runner = ai
                .run
                .Agent<Resolution>
                .new(
                    tools = [search_knowledge],
                    budget = ai.Budget { max_steps: 1, max_cost_usd: null },
                ),
        );
    log.info({ "handoff": handoff, "budget": budget });
    null
}
