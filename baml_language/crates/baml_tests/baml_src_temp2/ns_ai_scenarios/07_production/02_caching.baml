test "guide 07.02: managed cache lifetime is explicit" {
    let provider = root.ai.FakeCacheProvider {};
    let cache = root.ai.run.CreateCache.new(
        sample_history(),
        baml.time.Duration.from_minutes(5),
    ).run(provider);
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

function run_with_managed_cache(
    provider: root.google.Gemini,
) -> root.ai.Response<Resolution> throws baml.errors.UnknownError | baml.errors.Unsupported {
    // The repeated policy corpus is intentionally above Gemini's explicit-cache
    // minimum.
    let corpus = root.ai.MessageHistory {
        entries: [
            root.ai
                .ChatMessage
                .user(
                    "Support policy: duplicate card charges are usually pending authorizations. "
                        .repeat(800),
                ),
        ],
    };
    let cache = root.ai.run.CreateCache.new(
        corpus,
        baml.time.Duration.from_minutes(5),
    ).run(provider);
    cache.run<Resolution>(ResolveTicket_task(sample_ticket(), provider))
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
        let response = run_with_managed_cache(provider);
        assert.equal(provider.created_keys.length(), 1);
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
