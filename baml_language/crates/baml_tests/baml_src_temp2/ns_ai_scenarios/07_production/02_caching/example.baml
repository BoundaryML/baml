// Reuse a large support-policy corpus without resending it for every model call,
// then delete the remote cache when the work finishes.
function managed_cache() -> root.ai.ResponseWithMetadata<Resolution> {
    let provider = root.ai.FakeCacheProvider { inner: fake_resolution() };
    let task = ResolveTicket_task(sample_ticket(), provider);
    let response = task
        .run(
            runner = ManagedCacheRunner<Resolution>
                .new(_managed_cache_policy_corpus(), baml.time.Duration.from_minutes(5)),
        );
    log.info(response);
    response
}
