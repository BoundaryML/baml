// Record the model, tool, and terminal events emitted while a support agent
// resolves a ticket.
function events_and_callbacks() -> null {
    let observer = GuideObserver { kinds: [] };
    let trace = guide_agent_trace();
    let task = ResolveTicketWithTools_task(sample_ticket(), fake_tool_provider([one_knowledge_call()]), [search_knowledge]);
    let outcome = task
        .run(
            runner = root
                .ai
                .run
                .Agent<Resolution>
                .new(
                    tools = [search_knowledge],
                    after_tool_call = (event) -> {
                        trace.after_calls.push(event.call.name);
                        null
                    },
                    on_event = (event) -> {
                        trace.events.push(event.kind());
                        null
                    },
                    observers = [observer],
                ),
        );
    log.info(
        {
            "outcome": outcome,
            "observer": observer.kinds,
            "callbacks": trace.events,
            "tool_calls": trace.after_calls,
        },
    );
    null
}
