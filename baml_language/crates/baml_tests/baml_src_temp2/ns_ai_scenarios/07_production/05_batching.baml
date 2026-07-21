test "guide 07 extra: batch is another explicit resource driver" {
  let provider = root.ai.FakeBatchProvider { inner: fake_resolution() };
  let tasks = [
    ResolveTicket_task(sample_ticket(), provider),
    ResolveTicket_task(sample_ticket(), provider),
  ];
  let batch = root.ai.driver.batch<Resolution>(
    provider,
    root.ai.BatchOptions { idempotency_key: "batch-1" },
  ).drive(tasks);
  assert.equal(batch.results().length(), 2)
  batch.cleanup();
  batch.cleanup();
  assert.equal(batch.status(), root.ai.JobStatus.Cancelled)
}
