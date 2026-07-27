test "guide 02.05: before-tool callbacks may rewrite a call" {
    let trace = guide_agent_trace();
    let runner = root
        .ai
        .run
        .Agent<Resolution>
        .new(
            tools = [search_knowledge, lookup_account],
            before_tool_call = (event) -> {
                root.ai
                    .ToolDecision
                    .replace(
                        root.ai.ToolCall {
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
    task.run(runner = runner);
    assert.equal(trace.after_calls, ["lookup_account"])
}

test "guide 02.05: before-tool callbacks may block a call" {
    let trace = guide_agent_trace();
    let runner = root
        .ai
        .run
        .Agent<Resolution>
        .new(
            tools = [search_knowledge],
            before_tool_call = (event) -> {
                if (event.call.name == "search_knowledge") {
                    root.ai.ToolDecision.block("blocked by policy")
                } else {
                    root.ai.ToolDecision.allow(event.call)
                }
            },
            after_tool_call = (event) -> {
                trace.after_calls.push(event.call.name);
                null
            },
        );
    let task = ResolveTicketWithTools_task(sample_ticket(), fake_tool_provider([one_knowledge_call()]), [search_knowledge]);
    task.run(runner = runner);
    assert.equal(trace.after_calls.length(), 0)
}

function account_alias_tool() -> root.ai.Tool throws never {
    root.ai
        .tool(
            lookup_account,
            name = "search_knowledge",
            description = "Legacy account lookup alias. Call with customer_id.",
        )
}

testset "integ-test-guide-02-modify-or-block-tools" {
    test "guide 02.05 live OpenAI: a callback rewrites a real tool call before dispatch" {
        let trace = guide_agent_trace();
        let provider = live_openai();
        let task = ToolInstructions_task(
            provider,
            [account_alias_tool(), lookup_account],
            `Call search_knowledge exactly once with customer_id "C-1". After its result, return the final resolution.`,
        );
        let outcome = task
            .run(
                runner = root
                    .ai
                    .run
                    .Agent<Resolution>
                    .new(
                        tools = [account_alias_tool(), lookup_account],
                        before_tool_call = (event) -> {
                            root.ai
                                .ToolDecision
                                .replace(
                                    root.ai.ToolCall {
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
                    ),
            );
        log.info("live rewritten tool call " + { "after_calls": trace.after_calls }.to_string());
        assert.equal(trace.after_calls, ["lookup_account"]);
        match (outcome) {
            let done: root.ai.Done<Resolution> => assert.is_true(done.value.reply.length() > 0),
            _ => assert.is_true(false),
        }
    }

    test "guide 02.05 live OpenAI: a callback blocks a real tool call and returns an error result" {
        let trace = guide_agent_trace();
        let observer = GuideObserver { kinds: [] };
        let provider = live_openai();
        let task = ToolInstructions_task(
            provider,
            [search_knowledge],
            `Call search_knowledge once with query "duplicate charge". If policy blocks it, do not retry the tool; return a final resolution explaining that the lookup was unavailable.`,
        );
        let outcome = task
            .run(
                runner = root
                    .ai
                    .run
                    .Agent<Resolution>
                    .new(
                        tools = [search_knowledge],
                        before_tool_call = (event) -> {
                            if (event.call.name == "search_knowledge") {
                                root.ai.ToolDecision.block("blocked by policy")
                            } else {
                                root.ai.ToolDecision.allow(event.call)
                            }
                        },
                        after_tool_call = (event) -> {
                            trace.after_calls.push(event.call.name);
                            null
                        },
                        observers = [observer],
                    ),
            );
        log.info(
            "live blocked tool call "
                + { "after_calls": trace.after_calls, "events": observer.kinds }.to_string(),
        );
        assert.equal(trace.after_calls.length(), 0);
        assert.contains(observer.kinds.join(","), "tool_call_blocked");
        match (outcome) {
            let done: root.ai.Done<Resolution> => assert.is_true(done.value.reply.length() > 0),
            _ => assert.is_true(false),
        }
    }
}
