test "guide 01.05: a custom driver can call a known capability directly" {
  let provider = fake_resolution();
  let task = ResolveTicket_task(sample_ticket(), provider);
  let response = provider.generate<Resolution>(task);
  assert.equal(response.meta.provider, "fake")
  assert.equal(response.value.category, "billing")
}

