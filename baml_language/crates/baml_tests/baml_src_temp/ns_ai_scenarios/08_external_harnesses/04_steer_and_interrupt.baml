test "guide 08.04: steering and interruption target a concrete session" {
  let harness = fake_model_harness();
  let session = harness.open(harness_options());
  harness.steer(session, "Focus on billing code.");
  harness.interrupt(session);
  assert.is_true(session.stopped())
}

