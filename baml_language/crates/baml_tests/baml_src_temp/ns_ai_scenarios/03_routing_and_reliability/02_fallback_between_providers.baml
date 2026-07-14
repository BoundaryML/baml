test "guide 03.02: fallback re-renders and tries the next provider" {
  let failed = fake_resolution();
  failed.failures_remaining = 2;
  let good = fake_resolution();
  good.provider_name = "second";
  let response = root.ai.drivers.generate<Resolution>(
    ResolveTicket_task(sample_ticket(), root.ai.fallback([failed, good])),
  );
  assert.equal(response.meta.provider, "second")
}

testset "integ-test-guide-03-fallback" {
  test "guide 03.02 live Anthropic: fallback reaches a real second provider" {
    let providers: root.ai.Provider[] = [
      root.ai.OpenAi {
        model: "gpt-5.6-luna", api_key: "intentionally-invalid",
        base_url: null, extra_headers: null, extra_body: null,
      },
      root.ai.Anthropic {
        model: "claude-sonnet-4-6",
        api_key: baml.env.get_or_panic("ANTHROPIC_API_KEY"),
        base_url: null, extra_headers: null, extra_body: null,
      },
    ];
    let response = root.ai.drivers.generate<Resolution>(
      ResolveTicket_task(sample_ticket(), root.ai.fallback(providers)),
    );
    assert.equal(response.meta.provider, "anthropic")
  }
}

