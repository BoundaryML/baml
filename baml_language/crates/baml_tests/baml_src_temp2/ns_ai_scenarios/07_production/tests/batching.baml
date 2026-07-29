test "guide 07 extra: batch is another explicit resource runner" {
    let provider = root.ai.testing.FakeBatchProvider { inner: fake_resolution() };
    let tasks = [ResolveTicket_task(sample_ticket(), provider), ResolveTicket_task(sample_ticket(), provider)];
    let batch = root.ai.run.Batch<Resolution>.new(provider, idempotency_key = "batch-1").run(tasks);
    assert.equal(batch.results().length(), 2);
    // batch.cleanup();
    // batch.cleanup();
    // assert.equal(batch.status(), root.ai.jobs.JobStatus.Cancelled)
}
