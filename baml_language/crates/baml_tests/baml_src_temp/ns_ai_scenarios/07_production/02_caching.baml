test "guide 07.02: managed cache lifetime is explicit" {
  let provider = root.ai.FakeCacheProvider {};
  let cache = provider.create_cache(
    sample_conversation(),
    root.ai.CacheOptions { ttl: baml.time.Duration.from_minutes(5) },
  );
  assert.equal(cache.key(), "fake-cache-key")
  cache.cleanup();
  cache.cleanup();
  match (cache) {
    let fake: root.ai.FakeCache => assert.is_true(fake.deleted),
    _ => assert.is_true(false),
  }
}

testset "integ-test-guide-07-cache-task-path" {
  test "guide 07.02 live Anthropic: real provider accepts the shared task" {
    let value = ResolveTicket_manual(sample_ticket(), root.ai.Anthropic {
      ...root.ai.anthropic(),
      model: "claude-sonnet-4-6",
      api_key: baml.env.get_or_panic("ANTHROPIC_API_KEY"),
    });
    assert.is_true(value.reply.length() > 0)
  }
}
