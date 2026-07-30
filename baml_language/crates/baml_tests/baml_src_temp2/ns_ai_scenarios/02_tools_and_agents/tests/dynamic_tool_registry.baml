test "guide 02.06: a callback can add a tool before the next model step" {
    let registry = ai.tools.ToolRegistry.new([search_knowledge]);
    let account_tool = ai.tools.normalize_tool(lookup_account);
    let runner = ai.run.Agent<Resolution>.new(tools = [], tool_registry = registry, prepare_step = (context) -> {
        let next_tools = context.tools.slice(0, context.tools.length());
        if (context.step == 0) {
            next_tools.push(account_tool);
            ai.tools.StepPlan { provider: null, tools: next_tools, stop: null }
        } else {
            ai.tools.StepPlan { provider: null, tools: null, stop: null }
        }
    });
    let task = ResolveTicketWithTools@task(sample_ticket()).with_provider(fake_tool_provider([one_knowledge_call()])).with_tools([]);
    let outcome = task.run(runner = runner);
    match (outcome) {
        let done: ai.Done<Resolution> => match (done.conversation) {
            let conversation: ai.testing.FakeToolConversation => {
                assert.equal(conversation.last_tools.length(), 2)
            },
            _ => assert.is_true(false),
        },
        _ => assert.is_true(false),
    }
}

test "guide 02.06: duplicate tool names fail instead of shadowing" {
    let registry = ai.tools.ToolRegistry.new([search_knowledge]);
    let outcome = registry.add(search_knowledge) catch (e) {
        let collision: ai.Failure => "collision",
    };
    assert.equal(outcome, "collision");
    assert.equal(registry.snapshot().length(), 1)
}

test "guide 02.06: replacement is explicit" {
    let registry = ai.tools.ToolRegistry.new([search_knowledge]);
    let replacement = ai.tools.tool(search_knowledge, description = "Replacement handler contract.");
    assert.is_true(registry.replace(replacement));
    assert.equal(registry.snapshot()[0].description, "Replacement handler contract.")
}

testset "integ-test-guide-02-dynamic-tool-registry" {
    test "guide 02.06 live OpenAI: a tool added after turn one is used on turn two" {
        let registry = ai.tools.ToolRegistry.new([search_knowledge]);
        let trace = guide_agent_trace();
        let observer = GuideObserver { kinds: [] };
        let provider = live_openai();
        let task = ToolInstructions@task(`First call search_knowledge with query "duplicate charge". A lookup_account tool will be added after that result. Then call lookup_account with customer_id "C-1" before returning the final resolution.`).with_provider(provider).with_tools([]);
        let outcome = task
            .run(
                runner = ai
                    .run
                    .Agent<Resolution>
                    .new(
                        tools = [],
                        tool_registry = registry,
                        prepare_step = (context) -> {
                            let has_account = context
                                .tools
                                .filter((tool: ai.tools.Tool) -> bool {
                                    tool.name == "lookup_account"
                                })
                                .length()
                                > 0;
                            if (context.step == 1 && !has_account) {
                                let next_tools = context.tools.slice(0, context.tools.length());
                                next_tools.push(ai.tools.normalize_tool(lookup_account));
                                return ai.tools.StepPlan {
                                    provider: null,
                                    tools: next_tools,
                                    stop: null,
                                };
                            }
                            ai.tools.StepPlan { provider: null, tools: null, stop: null }
                        },
                        after_tool_call = (event) -> {
                            trace.after_calls.push(event.call.name);
                            null
                        },
                        observers = [observer],
                    ),
            );
        log.info(
            "live dynamic registry "
                + {
                    "calls": trace.after_calls,
                    "registry": registry.snapshot().map((tool: ai.tools.Tool) -> string {
                        tool.name
                    }),
                    "events": observer.kinds,
                }
                .to_string(),
        );
        assert.equal(trace.after_calls, ["search_knowledge", "lookup_account"]);
        assert.contains(observer.kinds.join(","), "tool_roster_changed");
        match (outcome) {
            let done: ai.Done<Resolution> => assert.is_true(done.value.reply.length() > 0),
            _ => assert.is_true(false),
        }
    }
}
