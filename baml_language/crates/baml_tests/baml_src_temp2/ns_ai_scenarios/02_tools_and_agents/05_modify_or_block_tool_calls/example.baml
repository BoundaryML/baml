// Enforce application policy by replacing a proposed search with an approved
// account lookup before any tool effect occurs.
function modify_or_block_tool_calls() -> null {
    let trace = guide_agent_trace();
    let runner = root
        .ai
        .run
        .Agent<Resolution>
        .new(
            tools = [search_knowledge, lookup_account],
            before_tool_call = (event) -> {
                root.ai
                    .tools
                    .ToolDecision
                    .replace(
                        root.ai.tools.ToolCall {
                            id: event.call.id,
                            name: "lookup_account",
                            args: event.call.args,
                        },
                    )
            },
            after_tool_call = (event) -> {
                trace.after_calls.push(event.call.name);
                null
            },
        );
    let task = ResolveTicketWithTools_task(
        sample_ticket(),
        fake_tool_provider([one_knowledge_call()]),
        [search_knowledge, lookup_account],
    );
    let outcome = task.run(runner = runner);
    log.info({ "outcome": outcome, "dispatched": trace.after_calls });
    null
}
