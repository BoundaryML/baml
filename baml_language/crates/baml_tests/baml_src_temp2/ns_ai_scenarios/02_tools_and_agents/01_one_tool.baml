// Answer a support ticket using information retrieved from one knowledge-base
// lookup.
function one_tool() -> null {
    let provider = fake_tool_provider([one_knowledge_call()]);
    let task = ResolveTicketWithTools_task(sample_ticket(), provider, [search_knowledge]);
    let outcome = task.run(runner = root.ai.run.Agent<Resolution>.new(tools = [search_knowledge]));
    log.info(outcome);
    null
}
