test "guide 08.05: harness sessions save and resume with opaque tokens" {
  let harness = fake_model_harness();
  let session = harness.open(harness_options());
  let token = harness.save_session(session);
  let restored = harness.restore_session(token);
  assert.equal(restored.id(), session.id())
}

