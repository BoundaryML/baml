test "guide 01.02: task construction is inspectable and side-effect free" {
  let fake = fake_resolution();
  let task = ResolveTicket_task(sample_ticket(), fake);
  assert.equal(fake.calls, 0)
  assert.equal(task.identity?.name, "ResolveTicket")
  assert.equal(task.provider_name(), "fake")
  let response = root.ai.drivers.generate_with_meta<Resolution>(task);
  assert.equal(response.value.category, "billing")
  assert.equal(fake.calls, 1)
}
