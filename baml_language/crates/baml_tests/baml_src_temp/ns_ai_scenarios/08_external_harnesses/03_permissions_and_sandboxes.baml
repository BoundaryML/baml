test "guide 08.03: permissions and sandbox are explicit open-session options" {
  let options = harness_options();
  assert.equal(options.permission_mode, "read-only")
  assert.equal(options.sandbox, "workspace")
  let session = fake_model_harness().open(options);
  assert.is_true(session.id().length() > 0)
}

