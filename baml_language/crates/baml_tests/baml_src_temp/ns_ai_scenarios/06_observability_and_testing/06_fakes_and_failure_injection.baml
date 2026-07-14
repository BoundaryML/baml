test "guide 06.06: fakes can inject failures before a valid typed output" {
  let fake = fake_resolution();
  fake.failures_remaining = 1;
  let failed = root.ai.drivers.generate<Resolution>(ResolveTicket_task(sample_ticket(), fake)) catch (e) {
    _ => "failed",
  };
  assert.equal(failed, "failed")
  assert.equal(root.ai.drivers.generate<Resolution>(ResolveTicket_task(sample_ticket(), fake)).value.category, "billing")
}

