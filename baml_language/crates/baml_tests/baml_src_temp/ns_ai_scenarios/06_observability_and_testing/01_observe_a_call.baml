test "guide 06.01: Response keeps value and provider metadata together" {
  let response = root.ai.drivers.generate_with_meta<Resolution>(
    ResolveTicket_task(sample_ticket(), fake_resolution()),
  );
  assert.equal(response.meta.provider, "fake")
  assert.equal(response.meta.request_id, "fake-1")
  assert.equal(response.value.category, "billing")
}
