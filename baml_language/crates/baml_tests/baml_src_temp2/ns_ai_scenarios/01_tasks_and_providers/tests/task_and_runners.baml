test "guide 01.02: task construction is inspectable and side-effect free" {
    let fake = fake_resolution();
    let task = ResolveTicket_task(sample_ticket(), fake);
    assert.equal(fake.calls, 0);
    assert.equal(task.identity?.name, "ResolveTicket");
    assert.equal(task.provider_name(), "fake");
    let response = task.run(
        runner = root.ai.run.CompletionWithMeta<Resolution>.new(),
    );
    assert.equal(response.value.category, "billing");
    assert.equal(fake.calls, 1)
}

testset "integ-test-guide-01-task-and-runners" {
    test "guide 01.02 live OpenAI: inspect the task before explicit execution" {
        let task = ResolveTicket_task(sample_ticket(), live_openai());
        assert.equal(task.identity?.name, "ResolveTicket");
        assert.equal(task.provider_name(), "openai");
        assert.contains(task.prompt.text(), "Charged twice");
        log.info(
            "live task constructed "
                + {
                    "identity": task.identity?.name,
                    "provider": task.provider_name(),
                    "tools": task.tools.length(),
                }
                .to_string(),
        );
        let response = task.run(
            runner = root.ai.run.CompletionWithMeta<Resolution>.new(),
        );
        log.info(
            "live task executed "
                + {
                    "provider": response.meta.provider,
                    "request_id": response.meta.request_id,
                    "usage": response.meta.usage,
                }
                .to_string(),
        );
        assert.equal(response.meta.provider, "openai");
        assert.is_true((response.meta.usage?.input_tokens ?? 0) > 0);
        assert.is_true(response.value.reply.length() > 0)
    }
}
