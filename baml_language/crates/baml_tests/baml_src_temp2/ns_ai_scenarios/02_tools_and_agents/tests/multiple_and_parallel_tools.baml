test "guide 02.03: one model step may dispatch multiple tool calls" {
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
    match (outcome) {
        let done: root.ai.Done<Resolution> => {
            assert.equal(done.value.priority, TicketPriority.Urgent)
        },
        _ => assert.is_true(false),
    }
}

function _invalid_tool_calls_are_rejected(calls: root.ai.tools.ToolCall[]) -> bool {
    let provider = root.ai.testing.ScriptedToolProvider {
        turns: [root.ai.tools.ToolCalls { calls: calls }],
        final_output: `
            {
                        "category":"billing",
                        "priority":"Urgent",
                        "summary":"unreachable",
                        "reply":"unreachable"
                    }
        `,
        usage_per_step: null,
    };
    let rejected = false;
    let _ = ResolveTicketWithTools_task(sample_ticket(), provider, [search_knowledge, lookup_account])
        .run(runner = root.ai.run.Agent<Resolution>.new(tools = [search_knowledge, lookup_account])) catch (e) {
        let failure: root.ai.Failure => {
            rejected = failure is root.ai.InvalidRequest;
            null
        },
    };
    rejected
}

test "guide 02.03: malformed provider tool-call envelopes fail before dispatch" {
    assert
        .is_true(
            _invalid_tool_calls_are_rejected(
                [
                    root.ai.tools.ToolCall {
                        id: "duplicate",
                        name: "search_knowledge",
                        args: { "query": "one" },
                    },
                    root.ai.tools.ToolCall {
                        id: "duplicate",
                        name: "search_knowledge",
                        args: { "query": "two" },
                    },
                ],
            ),
        );
    assert
        .is_true(
            _invalid_tool_calls_are_rejected(
                [
                    root.ai.tools.ToolCall {
                        id: "",
                        name: "search_knowledge",
                        args: { "query": "missing id" },
                    },
                ],
            ),
        );
    assert
        .is_true(
            _invalid_tool_calls_are_rejected([root.ai.tools.ToolCall { id: "missing-name", name: "", args: {} }]),
        );
    assert.is_true(_invalid_tool_calls_are_rejected([]))
}

testset "integ-test-guide-02-parallel-tools" {
    test "guide 02.03 live OpenAI: two tools are requested in one provider turn" {
        let trace = guide_agent_trace();
        let provider = live_openai();
        let task = ToolInstructions_task(
            provider,
            [search_knowledge, lookup_account],
            `Resolve ticket T-100. In your first response you must call both search_knowledge with query "duplicate charge" and lookup_account with customer_id "C-1" as parallel tool calls. Wait for both results before returning the final value.`,
        );
        let outcome = task
            .run(
                runner = root.ai.run.Agent<Resolution>.new(tools = [search_knowledge, lookup_account], after_tool_call = (
                    event,
                ) -> {
                    trace.after_calls.push(event.call.name);
                    null
                }),
            );
        match (outcome) {
            let done: root.ai.Done<Resolution> => match (done.conversation) {
                let conversation: root.openai.internal.Conversation => {
                    let parallel_turns = if (conversation.last_output.length() == 2) {
                        1
                    } else {
                        0
                    };
                    log.info(
                        "live parallel tools "
                            + { "dispatched": trace.after_calls, "parallel_turns": parallel_turns }
                                .to_string(),
                    );
                    assert.equal(parallel_turns, 1);
                    assert.equal(trace.after_calls.length(), 2);
                    assert.contains(trace.after_calls.join(","), "search_knowledge");
                    assert.contains(trace.after_calls.join(","), "lookup_account");
                },
                _ => assert.is_true(false),
            },
            _ => assert.is_true(false),
        }
    }
}
