test "guide 02.09: remove one application tool between provider steps" {
    let registry = ai.tools.ToolRegistry.new([search_knowledge, lookup_account]);
    let runner = ai.run.Agent<Resolution>.new(tools = [], tool_registry = registry, prepare_step = (context) -> {
        tool_roster_plan(context, remove_name = "lookup_account")
    });
    let task = ResolveTicketWithTools@task(sample_ticket()).with_provider(fake_tool_provider([one_knowledge_call()])).with_tools([]);
    let outcome = task.run(runner = runner);
    match (outcome) {
        let done: ai.Done<Resolution> => match (done.conversation) {
            let conversation: ai.testing.FakeToolConversation => {
                assert.equal(conversation.last_tools.length(), 1);
                assert.equal(conversation.last_tools[0].name, "search_knowledge");
            },
            _ => assert.is_true(false),
        },
        _ => assert.is_true(false),
    }
}

test "guide 02.09: an explicit empty list removes all application tools" {
    let runner = ai.run.Agent<Resolution>.new(tools = [search_knowledge, lookup_account], prepare_step = (context) -> {
        tool_roster_plan(context, remove_name = null, application_tools = [])
    });
    let task = ResolveTicketWithTools@task(sample_ticket()).with_provider(fake_tool_provider([one_knowledge_call()])).with_tools([search_knowledge, lookup_account]);
    let outcome = task.run(runner = runner);
    match (outcome) {
        let done: ai.Done<Resolution> => match (done.conversation) {
            let conversation: ai.testing.FakeToolConversation => {
                assert.equal(conversation.last_tools.length(), 0)
            },
            _ => assert.is_true(false),
        },
        _ => assert.is_true(false),
    }
}

test "guide 02.09: an empty replacement updates the registry and persists" {
    let registry = ai.tools.ToolRegistry.new([search_knowledge, lookup_account]);
    let runner = ai.run.Agent<Resolution>.new(tools = [], tool_registry = registry, prepare_step = (context) -> {
        tool_roster_plan(context, remove_name = null, clear_roster = true)
    });
    let task = ResolveTicketWithTools@task(sample_ticket()).with_provider(fake_tool_provider([one_knowledge_call()])).with_tools([]);
    let outcome = task.run(runner = runner);
    match (outcome) {
        let done: ai.Done<Resolution> => match (done.conversation) {
            let conversation: ai.testing.FakeToolConversation => {
                assert.equal(registry.snapshot().length(), 0);
                assert.equal(conversation.last_tools.length(), 0);
            },
            _ => assert.is_true(false),
        },
        _ => assert.is_true(false),
    }
}

test "guide 02.09: remove application and provider-owned tools together" {
    let web_search = ai.tools.ProviderTool {
        name: "web_search",
        kind: "openai.web_search",
        config: { "search_context_size": "low" },
    };
    let initial = fake_tool_provider_with_builtins([one_knowledge_call()], [web_search]);
    // Same provider display name and model behavior, different configuration.
    // A non-null StepPlan.provider must therefore switch even though name() is equal.
    let without_builtins = fake_tool_provider_with_builtins([], []);
    let runner = ai.run.Agent<Resolution>.new(tools = [search_knowledge], prepare_step = (context) -> {
        tool_roster_plan(context, remove_name = null, application_tools = [], next_provider = without_builtins)
    });
    let task = ResolveTicketWithTools@task(sample_ticket()).with_provider(initial).with_tools([search_knowledge]);
    let outcome = task.run(runner = runner);
    match (outcome) {
        let done: ai.Done<Resolution> => match (done.conversation) {
            let conversation: ai.testing.FakeToolConversation => {
                assert.equal(conversation.owner.name(), "fake-tools");
                assert.equal(conversation.last_tools.length(), 0);
                assert.equal(conversation.last_provider_tools.length(), 0);
            },
            _ => assert.is_true(false),
        },
        _ => assert.is_true(false),
    }
}

testset "integ-test-guide-02-remove-tools" {
    test "guide 02.09 live OpenAI: removing a tool changes the next provider request" {
        let registry = ai.tools.ToolRegistry.new([search_knowledge, lookup_account]);
        let observer = GuideObserver { kinds: [] };
        let provider = live_openai();
        let task = ToolInstructions@task(`First call search_knowledge with query "duplicate charge". After its result, lookup_account will be removed. Return the final resolution without calling a removed tool.`).with_provider(provider).with_tools([]);
        let outcome = task
            .run(
                runner = ai
                    .run
                    .Agent<Resolution>
                    .new(
                        tools = [],
                        tool_registry = registry,
                        prepare_step = (context) -> {
                            tool_roster_plan(context, remove_name = "lookup_account")
                        },
                        observers = [observer],
                    ),
            );
        match (outcome) {
            let done: ai.Done<Resolution> => match (done.conversation) {
                let conversation: root.openai.internal.Conversation => {
                    let names = conversation.active_tools.map((schema: json) -> string {
                        baml.json.path_or<string>(schema, ".name", "")
                    });
                    log.info(
                        "live removed tools "
                            + {
                                "registry": registry.snapshot().map((tool: ai.tools.Tool) -> string {
                                    tool.name
                                }),
                                "provider_tools": names,
                                "events": observer.kinds,
                            }
                            .to_string(),
                    );
                    assert.equal(names, ["search_knowledge"]);
                    assert.equal(registry.snapshot().length(), 1);
                    assert.contains(observer.kinds.join(","), "tool_roster_changed");
                },
                _ => assert.is_true(false),
            },
            _ => assert.is_true(false),
        }
    }
}
