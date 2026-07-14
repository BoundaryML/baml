test "guide 03.05: failure-aware switching is an explicit fallback policy" {
  let failed = fake_resolution();
  failed.failures_remaining = 1;
  let good = fake_resolution();
  let result = root.ai.drivers.generate<Resolution>(
    ResolveTicket_task(sample_ticket(), root.ai.fallback([failed, good])),
  );
  assert.equal(result.value.category, "billing")
}

