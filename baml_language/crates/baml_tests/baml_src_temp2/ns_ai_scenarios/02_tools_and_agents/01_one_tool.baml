// Answer a support ticket using information retrieved from one knowledge-base
// lookup.
function one_tool() -> null {
    let provider = fake_tool_provider([one_knowledge_call()]);
    let task = ResolveTicketWithTools@task(sample_ticket()).with_provider(provider).with_tools([search_knowledge]);
    let outcome = task.run(runner = ai.run.Agent<Resolution>.new(tools = [search_knowledge]));
    log.info(outcome);
    null
}
