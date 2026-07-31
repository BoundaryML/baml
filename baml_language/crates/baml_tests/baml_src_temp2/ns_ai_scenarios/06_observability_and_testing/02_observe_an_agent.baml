// Capture a chronological trace of model steps, tool activity, and termination
// while an agent works.
function observe_an_agent(
) -> string throws ai.Failure | baml.errors.UnknownError | baml.errors.Unsupported {
    let observer = GuideObserver { kinds: [] };
    let outcome = ResolveTicketWithTools@task(sample_ticket()).with_provider(fake_tool_provider([one_knowledge_call()])).with_tools([search_knowledge])
        .run(
            runner = ai.run.Agent<Resolution>.new(tools = [search_knowledge], observers = [observer]),
        );
    let result = match (outcome) {
        let done: ai.Done<Resolution> => done.value.reply,
        let budget: ai.Stopped => `budget reached: ${budget.reason}`,
        let handoff: ai.Handoff => `handoff: ${handoff.call.name}`,
        let interrupted: ai.Interrupted => `interrupted: ${interrupted.reason}`,
        let failed: ai.Failed => `failed at a committed checkpoint: ${failed.cause}`,
    };
    log.info({ "events": observer.kinds, "result": result });
    result
}
