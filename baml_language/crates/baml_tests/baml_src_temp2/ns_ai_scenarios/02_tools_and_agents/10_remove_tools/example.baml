// Revoke an agent's access to account data between model steps while leaving
// its remaining tools available.
function remove_tools() -> null {
    let registry = root.ai.ToolRegistry.new([search_knowledge, lookup_account]);
    let runner = root.ai.run.Agent<Resolution>.new(tools = [], tool_registry = registry, prepare_step = (context) -> {
        tool_roster_plan(context, remove_name = "lookup_account")
    });
    let task = ResolveTicketWithTools_task(sample_ticket(), fake_tool_provider([one_knowledge_call()]), []);
    let outcome = task.run(runner = runner);
    log.info({ "outcome": outcome, "remaining_tools": registry.snapshot() });
    null
}
