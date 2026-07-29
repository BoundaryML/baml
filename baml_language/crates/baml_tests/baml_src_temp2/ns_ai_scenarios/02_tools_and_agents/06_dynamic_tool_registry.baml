// Give an agent access to a newly available account tool without rebuilding the
// task or its original tool list.
function dynamic_tool_registry() -> null {
    let registry = root.ai.tools.ToolRegistry.new([search_knowledge]);
    let account_tool = root.ai.tools.normalize_tool(lookup_account);
    let runner = root.ai.run.Agent<Resolution>.new(tools = [], tool_registry = registry, prepare_step = (context) -> {
        let next_tools = context.tools.slice(0, context.tools.length());
        if (context.step == 0) {
            next_tools.push(account_tool);
            root.ai.tools.StepPlan { provider: null, tools: next_tools, stop: null }
        } else {
            root.ai.tools.StepPlan { provider: null, tools: null, stop: null }
        }
    });
    let task = ResolveTicketWithTools_task(sample_ticket(), fake_tool_provider([one_knowledge_call()]), []);
    let outcome = task.run(runner = runner);
    log.info({ "outcome": outcome, "tools": registry.snapshot() });
    null
}
