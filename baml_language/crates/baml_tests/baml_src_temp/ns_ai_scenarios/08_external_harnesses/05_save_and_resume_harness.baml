test "guide 08.05: harness sessions save and resume with opaque tokens" {
  let harness = fake_model_harness();
  let session = harness.open(harness_options());
  let token = harness.save_session(session);
  let restored = harness.restore_session(token);
  assert.equal(restored.id(), session.id())
  session.cleanup();
  session.cleanup();
  match (session) {
    let local: root.ai.LocalHarnessSession => assert.equal(local.cleanup_count, 1),
    _ => assert.is_true(false),
  }
}
