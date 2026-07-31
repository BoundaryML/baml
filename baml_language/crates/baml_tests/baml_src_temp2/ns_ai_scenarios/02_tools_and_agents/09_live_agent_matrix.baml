// Exercise dynamic tools, callbacks, and observation together in one complete
// agent workflow.
function live_agent_matrix() -> null {
    let registry = ai.tools.ToolRegistry.new([search_knowledge]);
    let trace = guide_agent_trace();
    let observer = GuideObserver { kinds: [] };
    let runner = ai
        .run
        .Agent<Resolution>
        .new(
            tool_registry = registry,
            prepare_step = (context) -> {
                if (context.step == 0) {
                    let next_tools = context.tools.slice(0, context.tools.length());
                    next_tools.push(ai.tools.normalize_tool(lookup_account));
                    ai.tools.StepPlan { provider: null, tools: next_tools, stop: null }
                } else {
                    ai.tools.StepPlan { provider: null, tools: null, stop: null }
                }
            },
            on_event = (event) -> {
                trace.events.push(event.kind());
                null
            },
            observers = [observer],
        );
    let task = ResolveTicketWithTools@task(sample_ticket()).with_provider(fake_tool_provider([one_knowledge_call()])).with_tools([]);
    let outcome = task.run(runner = runner);
    log.info({ "outcome": outcome, "events": trace.events, "observer": observer.kinds });
    null
}
