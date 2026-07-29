// Capture a chronological trace of model steps, tool activity, and termination
// while an agent works.
function observe_an_agent(
) -> string throws root.ai.Failure | baml.errors.UnknownError | baml.errors.Unsupported {
    let observer = GuideObserver { kinds: [] };
    let outcome = ResolveTicketWithTools_task(sample_ticket(), fake_tool_provider([one_knowledge_call()]), [search_knowledge])
        .run(
            runner = root.ai.run.Agent<Resolution>.new(tools = [search_knowledge], observers = [observer]),
        );
    let result = match (outcome) {
        let done: root.ai.Done<Resolution> => done.value.reply,
        let budget: root.ai.BudgetReached => `budget reached: ${budget.reason}`,
        let handoff: root.ai.Handoff => `handoff: ${handoff.call.name}`,
        let interrupted: root.ai.Interrupted => `interrupted: ${interrupted.reason}`,
    };
    log.info({ "events": observer.kinds, "result": result });
    result
}
