test "guide 08.04: steering and interruption target a concrete session" {
    let harness = fake_model_harness();
    let session = harness
        .open(
            ai.harness.HarnessOptions {
                cwd: "/workspace",
                permission_mode: "read-only",
                sandbox: "workspace",
                attributes: { "team": "support" },
            },
        );
    harness.steer(session, "Focus on billing code.");
    harness.interrupt(session);
    assert.is_true(session.stopped())
    // session.cleanup();
    // session.cleanup();
    // match (session) {
    //   let local: ai.LocalHarnessSession => assert.equal(local.cleanup_count, 1),
    //   _ => assert.is_true(false),
    // }
}

testset "integ-test-guide-08-harness-controls" {
    test "guide 08.04 live OpenAI: steering changes output and interruption blocks the next run" {
        let harness = live_openai_model_harness();
        let session = harness
            .open(
                ai.harness.HarnessOptions {
                    cwd: "/workspace",
                    permission_mode: "read-only",
                    sandbox: "workspace",
                    attributes: { "team": "support" },
                },
            );
        //# Steer this concrete session before its next provider call
        harness
            .steer(session, "The Resolution.reply field must include the exact token STEER-8427.");
        let steered = harness
            .run<Resolution>(
                session,
                InvestigateRepository@task(sample_ticket()).with_provider(harness.inner),
                null,
            );
        assert.contains(steered.value.reply, "STEER-8427");
        //# Interrupt retains the session but rejects subsequent work
        harness.interrupt(session);
        let blocked: ai.harness.HarnessRun<Resolution>? = harness
            .run<Resolution>(
                session,
                InvestigateRepository@task(sample_ticket()).with_provider(harness.inner),
                null,
            ) catch (e) {
            _ => null
        };
        log.info(
            baml.json
                .stringify(
                    {
                        "guide": "08.04",
                        "steering_applied": steered.value.reply.includes("STEER-8427"),
                        "stopped": session.stopped(),
                        "next_run_blocked": blocked == null,
                    },
                ),
        );
        assert.is_true(session.stopped());
        assert.equal(blocked, null)
    }
}
