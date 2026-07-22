test "guide 04.04: provider-owned sessions open, run, resume, and clean up" {
    let provider = root.ai.FakeSessionProvider { inner: fake_resolution(), sessions: [] };
    let session = root.ai.run.OpenSession.new(
        session_id = "support-T-100",
        attributes = { "team": "support" },
    ).run(provider);
    let response = session.run<Resolution>(ResolveTicket_task(sample_ticket(), provider.inner));
    assert.equal(response.value.category, "billing");
    let resumed = root.ai.run.ResumeSession.new(session.token()).run(provider);
    assert.equal(resumed.id(), session.id());
    session.cleanup();
    session.cleanup();
    match (session) {
        let fake: root.ai.FakeSession => assert.equal(fake.cleanup_count, 1),
        _ => assert.is_true(false),
    }
}

test "guide 04.04: session tokens validate provider ownership" {
    let provider = root.ai.FakeSessionProvider { inner: fake_resolution(), sessions: [] };
    let token = root.ai.SessionToken {
        provider: "another-provider",
        id: "missing",
        version: 1,
        sealed: "opaque",
    };
    let label = root.ai.run.ResumeSession.new(token).run(provider) catch (e) {
        let unknown: baml.errors.UnknownError => "rejected",
    };
    assert.equal(label, "rejected")
}

testset "integ-test-guide-04-provider-sessions" {
    test "guide 04.04 live OpenAI: session run, token resume, and idempotent cleanup" {
        let provider = root.ai.FakeSessionProvider { inner: live_openai(), sessions: [] };
        let session = root.ai.run.OpenSession.new(
            session_id = "live-support-T-100",
            attributes = { "team": "support" },
        ).run(provider);
        let response = session.run<Resolution>(ResolveTicket_task(sample_ticket(), provider.inner));
        let token = session.token();
        let resumed = root.ai.run.ResumeSession.new(token).run(provider);
        session.cleanup();
        session.cleanup();
        log.info(
            "live provider session "
                + {
                    "session_id": session.id(),
                    "resumed_id": resumed.id(),
                    "provider": response.meta.provider,
                }
                .to_string(),
        );
        assert.equal(response.meta.provider, "openai");
        assert.equal(resumed.id(), session.id());
        match (session) {
            let local: root.ai.FakeSession => assert.equal(local.cleanup_count, 1),
            _ => assert.is_true(false),
        }
    }
}
