test "guide 06.03: usage is aggregatable without becoming continuation state" {
  let first = root.ai.drivers.generate<Resolution>(ResolveTicket_task(sample_ticket(), fake_resolution()));
  let second = root.ai.drivers.generate<Resolution>(ResolveTicket_task(sample_ticket(), fake_resolution()));
  let usage = root.ai.aggregate_usage([first.meta, second.meta]);
  assert.equal(usage.input_tokens, 2)
  assert.equal(root.ai.estimated_cost(usage, root.ai.TokenPrice {
    input_per_million: 1.0,
    output_per_million: 2.0,
  }), 0.000006)
}

