test "guide 03.03: routing selects a provider before execution" {
  let fast = fake_resolution();
  fast.provider_name = "fast";
  let careful = fake_resolution();
  careful.provider_name = "careful";
  let task = ResolveTicket_task(sample_ticket(), fast);
  task.tags.set("tier", "pro");
  let routed = root.ai.route<Resolution>(task, TierRouter { fast: fast, careful: careful });
  assert.equal(routed.provider_name(), "careful")
}

