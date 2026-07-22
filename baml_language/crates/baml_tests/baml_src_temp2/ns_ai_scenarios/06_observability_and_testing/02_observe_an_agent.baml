test "guide 06.02: an observer receives model, tool, and completion events" {
    //# Attach an observer directly to the agent runner
    let observer = GuideObserver { kinds: [] };
    //# Run an observable tool loop
    ResolveTicketWithTools_task(sample_ticket(), fake_tool_provider([one_knowledge_call()]), [search_knowledge])
        .run(runner = root.ai.run.Agent<Resolution>.new(
            tools = [search_knowledge],
            observers = [observer],
        ));
    //# Verify model and completion events
    assert.contains(observer.kinds.join(","), "model_started");
    assert.contains(observer.kinds.join(","), "run_finished")
}

testset "integ-test-guide-06-observe-agent" {
    test "guide 06.02 live OpenAI: observer sees the exact model/tool/usage lifecycle" {
        let observer = GuideObserver { kinds: [] };
        let task = ToolInstructions_task(
            live_openai(),
            [search_knowledge],
            "Call search_knowledge exactly once for duplicate charges, then answer from its result.",
        );
        // The runner owns lifecycle policy; each Tool owns its AnyFunction handler.
        let outcome = task.run(runner = root.ai.run.Agent<Resolution>.new(
            tools = [search_knowledge],
            observers = [observer],
        ));
        match (outcome) {
            let done: root.ai.Done<Resolution> => assert.is_true(done.value.reply.length() > 0),
            _ => assert.is_true(false),
        }
        log.info(
            baml.json
                .stringify(
                    {
                        "guide": "06.02",
                        "event_count": observer.kinds.length(),
                        "events": observer.kinds.join(","),
                    },
                ),
        );
        assert
            .equal(
                observer.kinds,
                [
                    "model_started",
                    "usage_updated",
                    "tool_call_proposed",
                    "tool_call_started",
                    "tool_call_finished",
                    "model_started",
                    "usage_updated",
                    "run_finished",
                ],
            )
    }
}
