// Tests for the route_before_the_call scenario.
test "guide 03.03: routing selects a provider before execution" {
  let fast = root.ai.testing.FakeProvider { ...fake_resolution(), provider_name: "fast" };
  let careful = root.ai.testing.FakeProvider { ...fake_resolution(), provider_name: "careful" };
  let task = root.ai.Task<Resolution> {
    ...ResolveTicket_task(sample_ticket(), fast),
    tags: { "tier": "pro" },
  };
  let selected = if (task.tags.get("tier") == "pro") { careful } else { fast };
  let routed = task.with_provider(selected);
  assert.equal(routed.provider_name(), "careful")
}

testset "integ-test-guide-03-route-before-call" {
  test "guide 03.03 live Anthropic: routing selects the tagged provider before I/O" {
    let fast = live_openai();
    let careful = live_anthropic();
    let task = root.ai.Task<Resolution> {
      ...ResolveTicket_task(sample_ticket(), fast),
      tags: { "tier": "pro" },
    };
    let selected = if (task.tags.get("tier") == "pro") { careful } else { fast };
    let routed = task.with_provider(selected);
    assert.equal(routed.provider_name(), "anthropic")
    let response = routed.run(
      runner = root.ai.run.GenerationWithMeta<Resolution>.new(),
    );
    log.info("live route " + {
      "tag": task.tags.get("tier"),
      "selected": routed.provider_name(),
      "executed_by": response.metadata.provider,
    }.to_string());
    assert.equal(response.metadata.provider, "anthropic")
    assert.is_true(response.value.summary.length() > 0)
  }
}
