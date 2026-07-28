// Tests for the provider_owned_sessions scenario.
test "guide 04.04: provider-owned sessions open, run, and resume" {
    let provider = root.ai.FakeSessionProvider { inner: fake_resolution(), sessions: [] };
    let session = provider.open_session(root.ai.SessionOptions {
        session_id: "support-T-100",
        attributes: { "team": "support" },
    });
    let response = ResolveTicket_task(sample_ticket(), provider).run(
        runner = root.ai.run.InSession<Resolution>.new(session),
    );
    assert.equal(response.value.category, "billing");
    let resumed = provider.resume_session(session.token());
    assert.equal(resumed.id(), session.id());
    // session.cleanup();
    // session.cleanup();
    // match (session) {
    //     let fake: root.ai.FakeSession => assert.equal(fake.cleanup_count, 1),
    //     _ => assert.is_true(false),
    // }
}

test "guide 04.04: session tokens validate provider ownership" {
    let provider = root.ai.FakeSessionProvider { inner: fake_resolution(), sessions: [] };
    let token = root.ai.SessionToken {
        provider: "another-provider",
        id: "missing",
        version: 1,
        sealed: "opaque",
    };
    let label = provider.resume_session(token) catch (e) {
        let unknown: baml.errors.UnknownError => "rejected",
    };
    assert.equal(label, "rejected")
}

testset "integ-test-guide-04-provider-sessions" {
    test "guide 04.04 live OpenAI: session run and token resume" {
        let provider = root.ai.FakeSessionProvider { inner: live_openai(), sessions: [] };
        let session = provider.open_session(root.ai.SessionOptions {
            session_id: "live-support-T-100",
            attributes: { "team": "support" },
        });
        let response = ResolveTicket_task(sample_ticket(), provider).run(
            runner = root.ai.run.InSession<Resolution>.new(session),
        );
        let token = session.token();
        let resumed = provider.resume_session(token);
        // session.cleanup();
        // session.cleanup();
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
        // match (session) {
        //     let local: root.ai.FakeSession => assert.equal(local.cleanup_count, 1),
        //     _ => assert.is_true(false),
        // }
    }
}
