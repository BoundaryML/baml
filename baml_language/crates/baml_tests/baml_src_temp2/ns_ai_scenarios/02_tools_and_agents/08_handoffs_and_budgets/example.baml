// Return control to the application when work needs another specialist, and
// stop agent work cleanly when its step budget is exhausted.
function handoffs_and_budgets() -> null {
    let handoff_tool = root.ai.tool(lookup_account).as_handoff();
    let handoff_task = ResolveTicketWithTools_task(
        sample_ticket(),
        fake_tool_provider(
            [
                root.ai.ToolCall {
                    id: "handoff-1",
                    name: "lookup_account",
                    args: { "customer_id": "C-1" },
                },
            ],
        ),
        [handoff_tool],
    );
    let handoff = handoff_task.run(runner = root.ai.run.Agent<Resolution>.new(tools = [handoff_tool]));

    let budget_task = ResolveTicketWithTools_task(sample_ticket(), fake_tool_provider([one_knowledge_call()]), [search_knowledge]);
    let budget = budget_task
        .run(
            runner = root
                .ai
                .run
                .Agent<Resolution>
                .new(
                    tools = [search_knowledge],
                    budget = root.ai.Budget { max_steps: 1, max_cost_usd: null },
                ),
        );
    log.info({ "handoff": handoff, "budget": budget });
    null
}
