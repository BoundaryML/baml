test "guide 07 extra: batch is another explicit resource runner" {
    let provider = ai.testing.FakeBatchProvider { inner: fake_resolution() };
    let tasks = [ResolveTicket@task(sample_ticket()).with_provider(provider), ResolveTicket@task(sample_ticket()).with_provider(provider)];
    let batch = ai.run.Batch<Resolution>.new(provider, idempotency_key = "batch-1").run(tasks);
    assert.equal(batch.results().length(), 2);
    // batch.cleanup();
    // batch.cleanup();
    // assert.equal(batch.status(), ai.jobs.JobStatus.Cancelled)
}
