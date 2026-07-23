test "guide 02.04: observers and post-tool hooks see the run" {
    let observer = GuideObserver { kinds: [] };
    let hooks = no_op_hooks();
    let task = ResolveTicketWithTools_task(sample_ticket(), fake_tool_provider([one_knowledge_call()]), [search_knowledge]);
    let outcome = task.run(
        runner = root.ai.run.Agent<Resolution>.new(
            tools = [search_knowledge],
            hooks = hooks,
            observers = [observer],
        ),
    );
    assert.contains(observer.kinds.join(","), "tool_call_started");
    assert.equal(hooks.after_calls, ["search_knowledge"]);
    assert.contains(hooks.hook_events.join(","), "run_finished")
}

testset "integ-test-guide-02-events-and-hooks" {
    test "guide 02.04 live OpenAI: observer and hooks see the real tool lifecycle" {
        let observer = GuideObserver { kinds: [] };
        let hooks = no_op_hooks();
        let provider = live_openai();
        let task = ToolInstructions_task(
            provider,
            [search_knowledge],
            `Call search_knowledge exactly once with query "duplicate charge", then use its result to return the final resolution.`,
        );
        let outcome = task.run(
            runner = root.ai.run.Agent<Resolution>.new(
                tools = [search_knowledge],
                hooks = hooks,
                observers = [observer],
            ),
        );
        log.info(
            "live agent events "
                + {
                    "observer": observer.kinds,
                    "hook_events": hooks.hook_events,
                    "after_calls": hooks.after_calls,
                }
                .to_string(),
        );
        assert.equal(hooks.after_calls, ["search_knowledge"]);
        assert.contains(observer.kinds.join(","), "tool_call_proposed");
        assert.contains(observer.kinds.join(","), "tool_call_started");
        assert.contains(observer.kinds.join(","), "tool_call_finished");
        assert.contains(observer.kinds.join(","), "usage_updated");
        assert.contains(observer.kinds.join(","), "run_finished");
        match (outcome) {
            let done: root.ai.Done<Resolution> => assert.is_true(done.value.reply.length() > 0),
            _ => assert.is_true(false),
        }
    }
}
