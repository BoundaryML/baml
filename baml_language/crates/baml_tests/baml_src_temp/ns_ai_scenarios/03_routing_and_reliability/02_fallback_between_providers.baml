test "guide 03.02: fallback re-renders and tries the next provider" {
  let failed = root.ai.FakeProvider { ...fake_resolution(), failures_remaining: 2 };
  let good = root.ai.FakeProvider { ...fake_resolution(), provider_name: "second" };
  let response = root.ai.drivers.generate_with_meta<Resolution>(
    ResolveTicket_task(sample_ticket(), root.ai.fallback([failed, good])),
  );
  assert.equal(response.meta.provider, "second")
}

testset "integ-test-guide-03-fallback" {
  test "guide 03.02 live Anthropic: fallback reaches a real second provider" {
    let providers: root.ai.Provider[] = [
      {
        root.ai.FakeProvider { ...fake_resolution(), failures_remaining: 1 }
      },
      root.ai.Anthropic {
        ...root.ai.anthropic(),
        model: "claude-sonnet-4-6",
        api_key: baml.env.get_or_panic("ANTHROPIC_API_KEY"),
      },
    ];
    let response = root.ai.drivers.generate_with_meta<Resolution>(
      ResolveTicket_task(sample_ticket(), root.ai.fallback(providers)),
    );
    assert.equal(response.meta.provider, "anthropic")
  }
}
