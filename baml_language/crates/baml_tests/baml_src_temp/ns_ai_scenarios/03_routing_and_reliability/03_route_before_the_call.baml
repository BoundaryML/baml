test "guide 03.03: routing selects a provider before execution" {
  let fast = root.ai.FakeProvider { ...fake_resolution(), provider_name: "fast" };
  let careful = root.ai.FakeProvider { ...fake_resolution(), provider_name: "careful" };
  let task = root.ai.Task<Resolution> {
    ...ResolveTicket_task(sample_ticket(), fast),
    tags: { "tier": "pro" },
  };
  let routed = root.ai.route<Resolution>(task, TierRouter { fast: fast, careful: careful });
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
    let routed = root.ai.route<Resolution>(
      task,
      TierRouter { fast: fast, careful: careful },
    );
    assert.equal(routed.provider_name(), "anthropic")
    let response = root.ai.drivers.generate_with_meta<Resolution>(routed);
    log.info("live route " + {
      "tag": task.tags.get("tier"),
      "selected": routed.provider_name(),
      "executed_by": response.meta.provider,
    }.to_string());
    assert.equal(response.meta.provider, "anthropic")
    assert.is_true(response.value.summary.length() > 0)
  }
}
