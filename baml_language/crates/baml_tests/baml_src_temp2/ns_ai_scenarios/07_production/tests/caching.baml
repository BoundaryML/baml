test "guide 07.02: managed cache lifetime is explicit" {
    let provider = root.ai.FakeCacheProvider { inner: fake_resolution() };
    let cache = root.ai.run.CreateCache.new(sample_history(), baml.time.Duration.from_minutes(5)).run(provider);
    assert.equal(cache.key(), "fake-cache-key");
    assert
        .equal(
            cache
                .run<Resolution>(ResolveTicket_task(sample_ticket(), fake_resolution()))
                .value
                .category,
            "billing",
        );
    // cache.cleanup();
    // cache.cleanup();
    // match (cache) {
    //     let fake: root.ai.FakeCache => assert.is_true(fake.deleted),
    //     _ => assert.is_true(false),
    // }
}

test "guide 07.02: a custom runner owns the complete cache lifecycle" {
    let provider = root.ai.FakeCacheProvider { inner: fake_resolution() };
    let response = ResolveTicket_task(sample_ticket(), provider)
        .run(
            runner = ManagedCacheRunner<Resolution>.new(sample_history(), baml.time.Duration.from_minutes(5)),
        );
    assert.equal(response.value.category, "billing")
}

testset "integ-test-guide-07-managed-cache" {
    test "guide 07.02 live Gemini: create, use, and automatically delete a remote cache" {
        let provider = root.google.Gemini {
            model: "gemini-2.5-flash",
            api_key: baml.env.get_or_panic("GOOGLE_API_KEY"),
            base_url: null,
            created_keys: [],
            deleted_keys: [],
        };
        let response = ResolveTicket_task(sample_ticket(), provider)
            .run(
                runner = ManagedCacheRunner<Resolution>
                    .new(_managed_cache_policy_corpus(), baml.time.Duration.from_minutes(5)),
            );
        assert.equal(provider.created_keys.length(), 1);
        assert.equal(provider.deleted_keys.length(), 1);
        log.info(
            baml.json
                .stringify(
                    {
                        "guide": "07.02",
                        "cache_key": provider.created_keys[0],
                        "cached_input_tokens": response.meta.usage?.cached_input_tokens ?? 0,
                    },
                ),
        );
        assert.equal(response.meta.provider, "gemini-managed-cache");
        assert.is_true((response.meta.usage?.cached_input_tokens ?? 0) > 0);
        assert.is_true(response.value.reply.length() > 0)
    }
}
