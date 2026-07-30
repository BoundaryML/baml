test "guide 01.02: task construction is inspectable and side-effect free" {
    let fake = fake_resolution();
    let task = ResolveTicket@task(sample_ticket()).with_provider(fake);
    assert.equal(fake.calls, 0);
    assert.equal(task.identity?.name, "ResolveTicket");
    assert.equal(task.identity?.package, "user");
    assert.equal(task.provider_name(), "fake");
    let response = ai
        .internal
        ._response_from_done(
            ai.internal._require_done(task.run(runner = ai.run.Agent<Resolution>.new())),
        );
    assert.equal(response.value.category, "billing");
    assert.equal(fake.calls, 1)
}

testset "integ-test-guide-01-task-and-runners" {
    test "guide 01.02 live OpenAI: inspect the task before explicit execution" {
        let task = ResolveTicket@task(sample_ticket()).with_provider(live_openai());
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
        let response = ai
            .internal
            ._response_from_done(
                ai
                    .internal
                    ._require_done(task.run(runner = ai.run.Agent<Resolution>.new())),
            );
        log.info(
            "live task executed "
                + {
                    "provider": response.metadata.provider,
                    "request_id": response.metadata.request_id,
                    "usage": response.metadata.usage,
                }
                .to_string(),
        );
        assert.equal(response.metadata.provider, "openai");
        assert.is_true((response.metadata.usage?.input_tokens ?? 0) > 0);
        assert.is_true(response.value.reply.length() > 0)
    }
}
