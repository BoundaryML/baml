test "guide 07 extra: batch is another explicit resource driver" {
  let provider = root.ai.FakeBatchProvider { inner: fake_resolution() };
  let tasks = [
    ResolveTicket_task(sample_ticket(), provider),
    ResolveTicket_task(sample_ticket(), provider),
  ];
  let batch = root.ai.drivers.submit_batch<Resolution>(
    tasks,
    provider,
    root.ai.BatchOptions { idempotency_key: "batch-1" },
  );
  assert.equal(batch.results().length(), 2)
}
