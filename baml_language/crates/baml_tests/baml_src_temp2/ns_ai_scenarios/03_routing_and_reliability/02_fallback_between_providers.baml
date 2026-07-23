test "guide 03.02: fallback re-renders and tries the next provider" {
  //# Configure a failing primary and healthy fallback
  let failed = root.ai.FakeProvider { ...fake_resolution(), failures_remaining: 2 };
  let good = root.ai.FakeProvider { ...fake_resolution(), provider_name: "second" };
  //# Execute the ordered fallback chain
  let task = ResolveTicket_task(sample_ticket(), root.ai.fallback([failed, good]));
  let response = task.run(
    runner = root.ai.run.GenerationWithMeta<Resolution>.new(),
  );
  //# Verify the fallback provider completed
  assert.equal(response.meta.provider, "second")
}

testset "integ-test-guide-03-fallback" {
  test "guide 03.02 live Anthropic: fallback reaches a real second provider" {
    //# Configure a failing fake and live fallback
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
    //# Execute the live fallback chain
    let task = ResolveTicket_task(sample_ticket(), root.ai.fallback(providers));
    let response = task.run(
      runner = root.ai.run.GenerationWithMeta<Resolution>.new(),
    );
    //# Verify Anthropic completed the task
    assert.equal(response.meta.provider, "anthropic")
  }
}
