// Gather independent knowledge-base and account details in the same agent turn
// before answering the customer.
function multiple_and_parallel_tools() -> null {
    let calls = [
        one_knowledge_call(),
        root.ai.tools.ToolCall {
            id: "call-2",
            name: "lookup_account",
            args: { "customer_id": "C-1" },
        },
    ];
    let provider = fake_tool_provider(calls);
    let task = ResolveTicketWithTools_task(sample_ticket(), provider, [search_knowledge, lookup_account]);
    let outcome = task.run(runner = root.ai.run.Agent<Resolution>.new(tools = [search_knowledge, lookup_account]));
    log.info(outcome);
    null
}
