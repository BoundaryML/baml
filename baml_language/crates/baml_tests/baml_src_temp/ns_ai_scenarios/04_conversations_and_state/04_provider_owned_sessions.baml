test "guide 04.04: provider-owned sessions open, run, resume, and clean up" {
  let provider = root.ai.FakeSessionProvider { inner: fake_resolution(), sessions: [] };
  let session = root.ai.drivers.open_session(provider, root.ai.SessionOptions {
    session_id: "support-T-100",
    attributes: { "team": "support" },
  });
  let response = root.ai.drivers.run_in_session<Resolution>(
    session,
    ResolveTicket_task(sample_ticket(), provider.inner),
  );
  assert.equal(response.value.category, "billing")
  let resumed = root.ai.drivers.resume_session(provider, session.token());
  assert.equal(resumed.id(), session.id())
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
  let label = provider.resume_session(token) catch (e) {
    let unknown: baml.errors.UnknownError => "rejected",
  };
  assert.equal(label, "rejected")
}
