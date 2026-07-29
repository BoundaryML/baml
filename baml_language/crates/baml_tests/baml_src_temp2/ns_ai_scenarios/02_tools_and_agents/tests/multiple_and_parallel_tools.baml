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

class ToolCallLimitProbe {
    effects: int,

    function invoke(self, label: string) -> json throws never {
        self.effects = self.effects + 1;
        { "label": label }
    }
}

test "guide 02.03: Agent rejects an oversized tool batch before every application effect" {
    let probe = ToolCallLimitProbe { effects: 0 };
    let before_calls = 0;
    let after_calls = 0;
    let observer = GuideObserver { kinds: [] };
    let tool = root.ai.tools.tool(probe.invoke, name = "probe", description = "record one visible effect");
    let calls = [
        root.ai.tools.ToolCall { id: "limited-1", name: "probe", args: { "label": "one" } },
        root.ai.tools.ToolCall { id: "limited-2", name: "probe", args: { "label": "two" } },
    ];
    let provider = fake_tool_provider(calls);
    let task = ResolveTicketWithTools_task(sample_ticket(), provider, [tool]);
    let caught = false;
    let _ = task
        .run(
            runner = root
                .ai
                .run
                .Agent<Resolution>
                .new(
                    tools = [tool],
                    max_tool_calls_per_step = 1,
                    before_tool_call = (event) -> {
                        before_calls = before_calls + 1;
                        root.ai.tools.ToolDecision.allow(event.call)
                    },
                    after_tool_call = (event) -> {
                        after_calls = after_calls + 1;
                        null
                    },
                    observers = [observer],
                ),
        ) catch (e) {
        let limit: root.ai.ToolCallLimitExceeded => {
            caught = true;
            assert.equal(limit.provider, "fake-tools");
            assert.equal(limit.max_tool_calls_per_step, 1);
            assert.equal(limit.actual_tool_calls, 2);
            assert.equal(limit.steps_taken, 1);
            assert
                .equal(
                    limit.calls.calls.map((call) -> {
                        call.id
                    }),
                    ["limited-1", "limited-2"],
                );
            assert.equal(limit.effects(), root.ai.Effects.None);

            // The failed policy check retains the provider's pending calls and
            // conversation. Reject each call by its original correlation ID,
            // submit the results, then resume without replaying the model step.
            let continued = provider
                .submit(
                    limit.conversation,
                    limit.calls.calls.map((call) -> {
                        root.ai.tools.ToolResult.error(call, "parallel calls are disabled")
                    }),
                );
            let resumed = task
                .run(
                    runner = root
                        .ai
                        .run
                        .Agent<Resolution>
                        .new(tools = [tool], max_tool_calls_per_step = 1, conversation = continued),
                );
            match (resumed) {
                let done: root.ai.Done<Resolution> => {
                    assert.equal(done.value.summary, "Resolved with tools")
                },
                _ => assert.is_true(false),
            }
            null
        },
    };

    assert.is_true(caught);
    assert.equal(probe.effects, 0);
    assert.equal(before_calls, 0);
    assert.equal(after_calls, 0);
    assert.is_true(!observer.kinds.includes("tool_call"))
}

test "guide 02.03: null tool-call limit preserves parallel dispatch" {
    let probe = ToolCallLimitProbe { effects: 0 };
    let completed = 0;
    let tool = root.ai.tools.tool(probe.invoke, name = "probe", description = "record one visible effect");
    let provider = fake_tool_provider(
        [
            root.ai.tools.ToolCall { id: "unlimited-1", name: "probe", args: { "label": "one" } },
            root.ai.tools.ToolCall { id: "unlimited-2", name: "probe", args: { "label": "two" } },
        ],
    );
    let task = ResolveTicketWithTools_task(sample_ticket(), provider, [tool]);
    let outcome = task
        .run(
            runner = root.ai.run.Agent<Resolution>.new(tools = [tool], max_tool_calls_per_step = null, after_tool_call = (
                event,
            ) -> {
                completed = completed + 1;
                null
            }),
        );
    match (outcome) {
        let done: root.ai.Done<Resolution> => {
            assert.equal(done.value.summary, "Resolved with tools");
            assert.equal(completed, 2)
        },
        _ => assert.is_true(false),
    }
}

test "guide 02.03: a negative tool-call limit is rejected before the provider begins" {
    let provider = fake_tool_provider([one_knowledge_call()]);
    let task = ResolveTicketWithTools_task(sample_ticket(), provider, [search_knowledge]);
    let rejected = false;
    let _ = task
        .run(
            runner = root
                .ai
                .run
                .Agent<Resolution>
                .new(tools = [search_knowledge], max_tool_calls_per_step = -1),
        ) catch (e) {
        let invalid: root.ai.InvalidRequest => {
            rejected
                = invalid.detail == "Agent.max_tool_calls_per_step must be null or non-negative";
            null
        },
    };
    assert.is_true(rejected)
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
