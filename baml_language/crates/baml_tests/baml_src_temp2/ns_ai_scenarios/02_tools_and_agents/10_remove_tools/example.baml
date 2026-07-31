// Revoke an agent's access to account data between model steps while leaving
// its remaining tools available.
function remove_tools() -> null {
    let registry = ai.tools.ToolRegistry.new([search_knowledge, lookup_account]);
    let runner = ai.run.Agent<Resolution>.new(tool_registry = registry, prepare_step = (context) -> {
        tool_roster_plan(context, remove_name = "lookup_account")
    });
    let task = ResolveTicketWithTools@task(sample_ticket()).with_provider(fake_tool_provider([one_knowledge_call()])).with_tools([]);
    let outcome = task.run(runner = runner);
    log.info({ "outcome": outcome, "remaining_tools": registry.snapshot() });
    null
}
