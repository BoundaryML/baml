test "guide 02.08: a handoff root.ai.tools.tool returns control to the application" {
    let handoff_tool = root.ai.tools.tool(lookup_account).as_handoff();
    let provider = fake_tool_provider(
        [
            root.ai.tools.ToolCall {
                id: "handoff-1",
                name: "lookup_account",
                args: { "customer_id": "C-1" },
            },
        ],
    );
    let task = ResolveTicketWithTools_task(sample_ticket(), provider, [handoff_tool]);
    let outcome = task.run(runner = root.ai.run.Agent<Resolution>.new(tools = [handoff_tool]));
    match (outcome) {
        let handoff: root.ai.Handoff => {
            assert.equal(handoff.call.id, "handoff-1");
            assert.equal(handoff.call.name, "lookup_account");
            assert.equal(baml.json.path_or(handoff.call.args, ".customer_id", ""), "C-1");
            // The application completes the delegated work, preserves the
            // provider's call ID, and resumes the exact conversation.
            let continued = provider
                .submit(
                    handoff.conversation,
                    [
                        root.ai
                            .tools
                            .ToolResult
                            .ok(
                                handoff.call,
                                { "customer_id": "C-1", "status": "handled externally" },
                            ),
                    ],
                );
            let resumed = task
                .run(
                    runner = root
                        .ai
                        .run
                        .Agent<Resolution>
                        .new(tools = [handoff_tool], conversation = continued),
                );
            match (resumed) {
                let done: root.ai.Done<Resolution> => {
                    assert.equal(done.value.category, "billing")
                },
                _ => assert.is_true(false),
            }
        },
        _ => assert.is_true(false),
    }
}

test "guide 02.08: handoffs count toward max_tool_calls_per_step" {
    let handoff_tool = root.ai.tools.tool(lookup_account).as_handoff();
    let provider = fake_tool_provider(
        [
            root.ai.tools.ToolCall {
                id: "limited-handoff",
                name: "lookup_account",
                args: { "customer_id": "C-1" },
            },
        ],
    );
    let task = ResolveTicketWithTools_task(sample_ticket(), provider, [handoff_tool]);
    let rejected = false;
    let _ = task
        .run(
            runner = root.ai.run.Agent<Resolution>.new(tools = [handoff_tool], max_tool_calls_per_step = 0),
        ) catch (e) {
        let limit: root.ai.ToolCallLimitExceeded => {
            rejected = limit.actual_tool_calls == 1 && limit.max_tool_calls_per_step == 0;
            null
        },
    };
    assert.is_true(rejected)
}

function _ambiguous_handoff_is_rejected(
    calls: root.ai.tools.ToolCall[],
    tools: root.ai.tools.ToolInput[],
) -> bool {
    let provider = fake_tool_provider(calls);
    let rejected = false;
    let _ = ResolveTicketWithTools_task(sample_ticket(), provider, tools)
        .run(runner = root.ai.run.Agent<Resolution>.new(tools = tools)) catch (e) {
        let failure: root.ai.Failure => {
            rejected = failure is root.ai.InvalidRequest;
            null
        },
    };
    rejected
}

test "guide 02.08: handoff steps reject mixed or multiple pending calls" {
    let handoff_account = root.ai.tools.tool(lookup_account).as_handoff();
    let handoff_knowledge = root.ai.tools.tool(search_knowledge).as_handoff();
    assert
        .is_true(
            _ambiguous_handoff_is_rejected(
                [
                    root.ai.tools.ToolCall {
                        id: "handoff",
                        name: "lookup_account",
                        args: { "customer_id": "C-1" },
                    },
                    root.ai.tools.ToolCall {
                        id: "application",
                        name: "search_knowledge",
                        args: { "query": "duplicate charge" },
                    },
                ],
                [handoff_account, search_knowledge],
            ),
        );
    assert
        .is_true(
            _ambiguous_handoff_is_rejected(
                [
                    root.ai.tools.ToolCall {
                        id: "handoff-1",
                        name: "lookup_account",
                        args: { "customer_id": "C-1" },
                    },
                    root.ai.tools.ToolCall {
                        id: "handoff-2",
                        name: "search_knowledge",
                        args: { "query": "duplicate charge" },
                    },
                ],
                [handoff_account, handoff_knowledge],
            ),
        )
}

test "guide 02.08: a step budget returns resumable state" {
    let runner = root
        .ai
        .run
        .Agent<Resolution>
        .new(
            tools = [search_knowledge],
            budget = root.ai.Budget { max_steps: 1, max_cost_usd: null },
        );
    let task = ResolveTicketWithTools_task(sample_ticket(), fake_tool_provider([one_knowledge_call()]), [search_knowledge]);
    let outcome = task.run(runner = runner);
    match (outcome) {
        let stopped: root.ai.BudgetReached => {
            assert.equal(stopped.steps_taken, 1);
            assert.equal(stopped.reason, "max_steps");
        },
        _ => assert.is_true(false),
    }
}

test "guide 02.08: a cost budget stops between provider steps" {
    let provider = root.ai.testing.ScriptedToolProvider {
        turns: [root.ai.tools.ToolCalls { calls: [one_knowledge_call()] }],
        final_output: `
            {
                  "category":"billing",
                  "priority":"Urgent",
                  "summary":"not reached",
                  "reply":"not reached"
                }
        `,
        usage_per_step: root.ai.Usage {
            input_tokens: 10,
            output_tokens: 5,
            cached_input_tokens: 0,
            reasoning_tokens: 0,
            cost_usd: 0.02,
        },
    };
    let runner = root
        .ai
        .run
        .Agent<Resolution>
        .new(
            tools = [search_knowledge],
            budget = root.ai.Budget { max_steps: null, max_cost_usd: 0.01 },
        );
    let task = ResolveTicketWithTools_task(sample_ticket(), provider, [search_knowledge]);
    let outcome = task.run(runner = runner);
    match (outcome) {
        let stopped: root.ai.BudgetReached => {
            assert.equal(stopped.steps_taken, 1);
            assert.equal(stopped.reason, "max_cost_usd");
        },
        _ => assert.is_true(false),
    }
}

testset "integ-test-guide-02-budgets" {
    test "guide 02.08 live OpenAI: step budget stops after one real provider turn" {
        let provider = live_openai();
        let runner = root
            .ai
            .run
            .Agent<Resolution>
            .new(
                tools = [search_knowledge],
                budget = root.ai.Budget { max_steps: 1, max_cost_usd: null },
            );
        let task = ToolInstructions_task(
            provider,
            [search_knowledge],
            `Call search_knowledge with query "duplicate charge" before returning a final resolution.`,
        );
        let outcome = task.run(runner = runner);
        match (outcome) {
            let stopped: root.ai.BudgetReached => {
                log.info(
                    "live step budget "
                        + { "steps": stopped.steps_taken, "reason": stopped.reason }.to_string(),
                );
                assert.equal(stopped.steps_taken, 1);
                assert.equal(stopped.reason, "max_steps");
            },
            _ => assert.is_true(false),
        }
    }

    test "guide 02.08 live OpenAI: cost budget stops after metered real provider usage" {
        let provider = LiveCostedToolProvider { inner: live_openai(), cost_per_step_usd: 0.02 };
        let runner = root
            .ai
            .run
            .Agent<Resolution>
            .new(
                tools = [search_knowledge],
                budget = root.ai.Budget { max_steps: null, max_cost_usd: 0.01 },
            );
        let task = ToolInstructions_task(
            provider,
            [search_knowledge],
            `Call search_knowledge with query "duplicate charge" before returning a final resolution.`,
        );
        let outcome = task.run(runner = runner);
        match (outcome) {
            let stopped: root.ai.BudgetReached => {
                log.info(
                    "live cost budget "
                        + { "steps": stopped.steps_taken, "reason": stopped.reason }.to_string(),
                );
                assert.equal(stopped.steps_taken, 1);
                assert.equal(stopped.reason, "max_cost_usd");
            },
            _ => assert.is_true(false),
        }
    }
}
