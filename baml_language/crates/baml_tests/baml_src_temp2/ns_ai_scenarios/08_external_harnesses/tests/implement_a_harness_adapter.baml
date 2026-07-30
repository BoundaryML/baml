test "guide 08.06: a custom harness composes through out-of-body implements" {
    let harness = NamedHarness { inner: fake_model_harness() };
    let run = InvestigateRepository@task(sample_ticket()).with_provider(harness.inner.inner)
        .run(
            runner = ai
                .run
                .Harness<Resolution>
                .new(
                    harness,
                    cwd = "/workspace",
                    permission_mode = "read-only",
                    sandbox = "workspace",
                    attributes = { "team": "support" },
                ),
        );
    assert.equal(harness.label(), "custom-coding-harness");
    assert.equal(run.value.category, "billing")
}

testset "integ-test-guide-08-custom-harness" {
    test "guide 08.06 live OpenAI: out-of-body adapter preserves typed execution" {
        let inner = live_openai_model_harness();
        let harness = NamedHarness { inner: inner };
        //# Execute a real provider through the custom out-of-body Harness impl
        let run = InvestigateRepository@task(sample_ticket()).with_provider(inner.inner)
            .run(
                runner = ai
                    .run
                    .Harness<Resolution>
                    .new(
                        harness,
                        cwd = "/workspace",
                        permission_mode = "read-only",
                        sandbox = "workspace",
                        attributes = { "team": "support" },
                    ),
            );
        log.info(
            baml.json
                .stringify(
                    {
                        "guide": "08.06",
                        "adapter": harness.label(),
                        "runtime": run.token.runtime,
                        "event_count": run.events.length(),
                        "result_length": run.value.reply.length(),
                    },
                ),
        );
        assert.equal(harness.label(), "custom-coding-harness");
        assert.equal(run.token.runtime, "baml");
        assert.is_true(run.events.length() >= 2);
        assert.is_true(run.value.reply.length() > 0)
    }
}
