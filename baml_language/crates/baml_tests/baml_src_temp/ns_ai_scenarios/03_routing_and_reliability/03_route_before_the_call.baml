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
