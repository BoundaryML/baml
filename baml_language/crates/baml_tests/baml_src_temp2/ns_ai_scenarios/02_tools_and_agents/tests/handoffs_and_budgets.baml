test "guide 02.08: a handoff ai.tools.tool returns control to the application" {
    let handoff_tool = ai.tools.tool(lookup_account).as_handoff();
    let provider = fake_tool_provider(
        [
            ai.tools.ToolCall {
                id: "handoff-1",
                name: "lookup_account",
                args: { "customer_id": "C-1" },
            },
        ],
    );
    let task = ResolveTicketWithTools@task(sample_ticket()).with_provider(provider).with_tools([handoff_tool]);
    let outcome = task.run(runner = ai.run.Agent<Resolution>.new(tools = [handoff_tool]));
    match (outcome) {
        let handoff: ai.Handoff => {
            assert.equal(handoff.call.id, "handoff-1");
            assert.equal(handoff.call.name, "lookup_account");
            assert.equal(baml.json.path_or(handoff.call.args, ".customer_id", ""), "C-1");
            // The application completes the delegated work; the session
            // submits the correlated result and continues the same turn.
            let session = ai.run.AgentSession<Resolution>.of(task, outcome);
            let resumed = session.submit_tool_results(
                [
                    ai
                        .tools
                        .ToolOk
                        .of(
                            handoff.call,
                            { "customer_id": "C-1", "status": "handled externally" },
                        ),
                ],
                runner = ai.run.Agent<Resolution>.new(tools = [handoff_tool]),
            );
            match (resumed) {
                let done: ai.Done<Resolution> => {
                    assert.equal(done.value.category, "billing")
                },
                _ => assert.is_true(false),
            }
        },
        _ => assert.is_true(false),
    }
}

test "guide 02.08: handoffs count toward max_tool_calls_per_step" {
    let handoff_tool = ai.tools.tool(lookup_account).as_handoff();
    let provider = fake_tool_provider(
        [
            ai.tools.ToolCall {
                id: "limited-handoff",
                name: "lookup_account",
                args: { "customer_id": "C-1" },
            },
        ],
    );
    let task = ResolveTicketWithTools@task(sample_ticket()).with_provider(provider).with_tools([handoff_tool]);
    //# A committed step with a rejected batch is an involuntary stop, not a
    //# throw: the Failed outcome carries the cause and the resumable state.
    let outcome = task
        .run(
            runner = ai.run.Agent<Resolution>.new(tools = [handoff_tool], max_tool_calls_per_step = 0),
        );
    let rejected = match (outcome) {
        let failed: ai.Failed => {
            match (failed.cause) {
                let limit: ai.ToolCallLimitExceeded => {
                    limit.actual_tool_calls == 1 && limit.max_tool_calls_per_step == 0
                },
                _ => false,
            }
        },
        _ => false,
    };
    assert.is_true(rejected)
}

function _ambiguous_handoff_is_rejected(
    calls: ai.tools.ToolCall[],
    tools: ai.tools.ToolInput[],
) -> bool {
    let provider = fake_tool_provider(calls);
    let outcome = ResolveTicketWithTools@task(sample_ticket()).with_provider(provider).with_tools(tools)
        .run(runner = ai.run.Agent<Resolution>.new(tools = tools));
    match (outcome) {
        let failed: ai.Failed => {
            match (failed.cause) {
                let invalid: ai.InvalidRequest => true,
                _ => false,
            }
        },
        _ => false,
    }
}

test "guide 02.08: handoff steps reject mixed or multiple pending calls" {
    let handoff_account = ai.tools.tool(lookup_account).as_handoff();
    let handoff_knowledge = ai.tools.tool(search_knowledge).as_handoff();
    assert
        .is_true(
            _ambiguous_handoff_is_rejected(
                [
                    ai.tools.ToolCall {
                        id: "handoff",
                        name: "lookup_account",
                        args: { "customer_id": "C-1" },
                    },
                    ai.tools.ToolCall {
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
                    ai.tools.ToolCall {
                        id: "handoff-1",
                        name: "lookup_account",
                        args: { "customer_id": "C-1" },
                    },
                    ai.tools.ToolCall {
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
    let runner = ai
        .run
        .Agent<Resolution>
        .new(
            tools = [search_knowledge],
            max_steps = 1,
        );
    let task = ResolveTicketWithTools@task(sample_ticket()).with_provider(fake_tool_provider([one_knowledge_call()])).with_tools([search_knowledge]);
    let outcome = task.run(runner = runner);
    match (outcome) {
        let stopped: ai.Stopped => {
            assert.equal(stopped.steps_taken, 1);
            assert.equal(stopped.reason, "max_steps");
        },
        _ => assert.is_true(false),
    }
}

test "guide 02.08: a stop_when policy stops between provider steps" {
    let provider = ai.testing.ScriptedToolProvider {
        turns: [ai.tools.ToolCalls { calls: [one_knowledge_call()] }],
        final_output: `
            {
                  "category":"billing",
                  "priority":"Urgent",
                  "summary":"not reached",
                  "reply":"not reached"
                }
        `,
        usage_per_step: ai.Usage {
            input_tokens: 10,
            output_tokens: 5,
            cached_input_tokens: 0,
            reasoning_tokens: 0,
        },
    };
    //# The application owns spend policy: price the tokens and stop the run
    //# when the estimate crosses its cap.
    let price = ai.observe.TokenPrice { input_per_million: 1000.0, output_per_million: 1000.0 };
    let runner = ai
        .run
        .Agent<Resolution>
        .new(
            tools = [search_knowledge],
            max_steps = null,
            stop_when = (context: ai.tools.StepContext) -> bool {
                ai.observe.estimated_cost(context.usage, price) > 0.01
            },
        );
    let task = ResolveTicketWithTools@task(sample_ticket()).with_provider(provider).with_tools([search_knowledge]);
    let outcome = task.run(runner = runner);
    match (outcome) {
        let stopped: ai.Stopped => {
            assert.equal(stopped.steps_taken, 1);
            assert.equal(stopped.reason, "stop_when");
        },
        _ => assert.is_true(false),
    }
}

testset "integ-test-guide-02-budgets" {
    test "guide 02.08 live OpenAI: step budget stops after one real provider turn" {
        let provider = live_openai();
        let runner = ai
            .run
            .Agent<Resolution>
            .new(
                tools = [search_knowledge],
                max_steps = 1,
            );
        let task = ToolInstructions@task(`Call search_knowledge with query "duplicate charge" before returning a final resolution.`).with_provider(provider).with_tools([search_knowledge]);
        let outcome = task.run(runner = runner);
        match (outcome) {
            let stopped: ai.Stopped => {
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

    test "guide 02.08 live OpenAI: stop_when policy stops after metered real provider usage" {
        let provider = live_openai();
        //# Any real step reports nonzero tokens, so this policy trips after
        //# exactly one live provider turn.
        let price = ai.observe.TokenPrice { input_per_million: 1.0, output_per_million: 2.0 };
        let runner = ai
            .run
            .Agent<Resolution>
            .new(
                tools = [search_knowledge],
                max_steps = null,
                stop_when = (context: ai.tools.StepContext) -> bool {
                    ai.observe.estimated_cost(context.usage, price) > 0.0
                },
            );
        let task = ToolInstructions@task(`Call search_knowledge with query "duplicate charge" before returning a final resolution.`).with_provider(provider).with_tools([search_knowledge]);
        let outcome = task.run(runner = runner);
        match (outcome) {
            let stopped: ai.Stopped => {
                log.info(
                    "live stop_when "
                        + { "steps": stopped.steps_taken, "reason": stopped.reason }.to_string(),
                );
                assert.equal(stopped.steps_taken, 1);
                assert.equal(stopped.reason, "stop_when");
            },
            _ => assert.is_true(false),
        }
    }
}
