test "guide 02.04: observers and agent callbacks see the run" {
    let observer = GuideObserver { kinds: [] };
    let trace = guide_agent_trace();
    let task = ResolveTicketWithTools_task(sample_ticket(), fake_tool_provider([one_knowledge_call()]), [search_knowledge]);
    let outcome = task
        .run(
            runner = root
                .ai
                .run
                .Agent<Resolution>
                .new(
                    tools = [search_knowledge],
                    after_tool_call = (event) -> {
                        trace.after_calls.push(event.call.name);
                        null
                    },
                    on_event = (event) -> {
                        trace.events.push(event.kind());
                        null
                    },
                    observers = [observer],
                ),
        );
    assert.contains(observer.kinds.join(","), "tool_call_started");
    assert.equal(trace.after_calls, ["search_knowledge"]);
    assert.contains(trace.events.join(","), "run_finished")
}

testset "integ-test-guide-02-events-and-callbacks" {
    test "guide 02.04 live OpenAI: observer and callbacks see the real tool lifecycle" {
        let observer = GuideObserver { kinds: [] };
        let trace = guide_agent_trace();
        let provider = live_openai();
        let task = ToolInstructions_task(
            provider,
            [search_knowledge],
            `Call search_knowledge exactly once with query "duplicate charge", then use its result to return the final resolution.`,
        );
        let outcome = task
            .run(
                runner = root
                    .ai
                    .run
                    .Agent<Resolution>
                    .new(
                        tools = [search_knowledge],
                        after_tool_call = (event) -> {
                            trace.after_calls.push(event.call.name);
                            null
                        },
                        on_event = (event) -> {
                            trace.events.push(event.kind());
                            null
                        },
                        observers = [observer],
                    ),
            );
        log.info(
            "live agent events "
                + {
                    "observer": observer.kinds,
                    "callback_events": trace.events,
                    "after_calls": trace.after_calls,
                }
                .to_string(),
        );
        assert.equal(trace.after_calls, ["search_knowledge"]);
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
