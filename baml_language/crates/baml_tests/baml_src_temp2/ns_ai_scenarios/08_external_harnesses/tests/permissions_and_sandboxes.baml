test "guide 08.03: permissions and sandbox are explicit open-session options" {
    let options = root.ai.harness.HarnessOptions {
        cwd: "/workspace",
        permission_mode: "read-only",
        sandbox: "workspace",
        attributes: { "team": "support" },
    };
    assert.equal(options.permission_mode, "read-only");
    assert.equal(options.sandbox, "workspace");
    let session = fake_model_harness().open(options);
    assert.is_true(session.id().length() > 0);
    match (session) {
        let local: root.ai.harness.LocalHarnessSession => {
            assert.equal(local.options.permission_mode, "read-only");
            assert.equal(local.options.sandbox, "workspace")
        },
        _ => assert.is_true(false),
    }
}

testset "integ-test-guide-08-harness-isolation" {
    test "guide 08.03 live OpenAI: permission and sandbox policy stay attached to the session" {
        let harness = live_openai_model_harness();
        let options = root.ai.harness.HarnessOptions {
            cwd: "/workspace",
            permission_mode: "read-only",
            sandbox: "workspace",
            attributes: { "team": "support", "network": "provider-only" },
        };
        //# Open a session with two distinct isolation controls
        let session = harness.open(options);
        let local = match (session) {
            let value: root.ai.harness.LocalHarnessSession => value,
            _ => baml.sys.panic("expected local model harness session"),
        };
        assert.equal(local.options.permission_mode, "read-only");
        assert.equal(local.options.sandbox, "workspace");
        assert.equal(local.options.attributes.get("network"), "provider-only");
        //# A read-only investigation still performs a real model run
        let run = harness
            .run<Resolution>(
                session,
                InvestigateRepository_task(sample_ticket(), harness.inner),
                null,
            );
        log.info(
            baml.json
                .stringify(
                    {
                        "guide": "08.03",
                        "session_id": session.id(),
                        "permission_mode": local.options.permission_mode,
                        "sandbox": local.options.sandbox,
                        "result_length": run.value.reply.length(),
                    },
                ),
        );
        assert.is_true(run.value.reply.length() > 0)
    }
}
