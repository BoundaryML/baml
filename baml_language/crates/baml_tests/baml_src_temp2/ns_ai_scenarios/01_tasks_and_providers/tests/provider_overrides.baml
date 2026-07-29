test "guide 01.04: changing providers re-renders the same task recipe" {
  let fast = root.ai.testing.FakeProvider {
    ...root.ai.testing.fake_output_provider(`{
      "category":"billing","priority":"Normal","summary":"fast","reply":"fast reply"
    }`),
    provider_name: "fast",
  };
  let careful = root.ai.testing.FakeProvider {
    ...fake_resolution(),
    provider_name: "careful",
  };
  let first = ResolveTicket_task(sample_ticket(), fast);
  let second = first.with_provider(careful);
  assert.equal(first.provider_name(), "fast")
  assert.equal(second.provider_name(), "careful")
  assert.contains(second.prompt.text(), "Charged twice")
}

testset "integ-test-guide-01-provider-overrides" {
  test "guide 01.04 live Anthropic: overriding rebinds and executes the new provider" {
    let original = ResolveTicket_task(sample_ticket(), live_openai());
    let overridden = original.with_provider(live_anthropic());
    assert.equal(original.provider_name(), "openai")
    assert.equal(overridden.provider_name(), "anthropic")
    assert.contains(overridden.prompt.text(), "Charged twice")
    let response = root.ai.internal._response_from_done(
      root.ai.internal._require_done(
        overridden.run(runner = root.ai.run.Agent<Resolution>.new()),
      ),
    );
    log.info("live provider override " + {
      "before": original.provider_name(),
      "after": overridden.provider_name(),
      "executed_by": response.metadata.provider,
    }.to_string());
    assert.equal(response.metadata.provider, "anthropic")
    assert.is_true(response.value.summary.length() > 0)
  }
}
