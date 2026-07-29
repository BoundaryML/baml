test "guide 08.02: one harness run delivers callbacks and preserves its event history" {
    let harness = fake_model_harness();
    let observed: string[] = [];
    let run = InvestigateRepository_task(sample_ticket(), harness.inner)
        .run(
            runner = root.ai.run.Harness<Resolution>.new(harness, cwd = "/workspace", permission_mode = "read-only", sandbox = "workspace", attributes = { "team": "support" }, on_event = (
                event: root.ai.observe.AgentEvent,
            ) -> null {
                observed.push(event.kind());
                null
            }),
        );
    assert
        .equal(
            run.events.map((event: root.ai.observe.AgentEvent) -> string {
                event.kind()
            }),
            ["model_started", "usage_updated", "run_finished"],
        );
    assert.equal(observed, ["model_started", "usage_updated", "run_finished"])
}

testset "integ-test-guide-08-harness-events" {
    test "guide 08.02 live OpenAI: callbacks arrive during the run and remain available afterward" {
        let harness = live_openai_model_harness();
        let kinds: string[] = [];
        //# Observe normalized events without changing the runner or result type
        let run = InvestigateRepository_task(sample_ticket(), harness.inner)
            .run(
                runner = root.ai.run.Harness<Resolution>.new(harness, cwd = "/workspace", permission_mode = "read-only", sandbox = "workspace", attributes = { "team": "support" }, on_event = (
                    event: root.ai.observe.AgentEvent,
                ) -> null {
                    kinds.push(event.kind());
                    log.info("harness event: " + event.kind());
                    null
                }),
            );
        let retained = run.events.map((event: root.ai.observe.AgentEvent) -> string {
            event.kind()
        });
        log.info(
            baml.json
                .stringify(
                    {
                        "guide": "08.02",
                        "event_kinds": kinds.join(","),
                        "terminal_event": run.events[run.events.length() - 1].kind(),
                    },
                ),
        );
        assert.equal(kinds[0], "model_started");
        assert.equal(kinds, retained);
        assert.equal(run.events[run.events.length() - 1].kind(), "run_finished");
        assert.is_true(run.value.reply.length() > 0)
    }
}
