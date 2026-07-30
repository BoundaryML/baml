// Give an agent access to a newly available account tool without rebuilding the
// task or its original tool list.
function dynamic_tool_registry() -> null {
    let registry = ai.tools.ToolRegistry.new([search_knowledge]);
    let account_tool = ai.tools.normalize_tool(lookup_account);
    let runner = ai.run.Agent<Resolution>.new(tools = [], tool_registry = registry, prepare_step = (context) -> {
        let next_tools = context.tools.slice(0, context.tools.length());
        if (context.step == 0) {
            next_tools.push(account_tool);
            ai.tools.StepPlan { provider: null, tools: next_tools, stop: null }
        } else {
            ai.tools.StepPlan { provider: null, tools: null, stop: null }
        }
    });
    let task = ResolveTicketWithTools@task(sample_ticket()).with_provider(fake_tool_provider([one_knowledge_call()])).with_tools([]);
    let outcome = task.run(runner = runner);
    log.info({ "outcome": outcome, "tools": registry.snapshot() });
    null
}
