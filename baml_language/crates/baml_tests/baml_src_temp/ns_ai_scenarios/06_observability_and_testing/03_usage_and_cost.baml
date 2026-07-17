test "guide 06.03: usage is aggregatable without becoming continuation state" {
  let first = root.ai.drivers.generate_with_meta<Resolution>(ResolveTicket_task(sample_ticket(), fake_resolution()));
  let second = root.ai.drivers.generate_with_meta<Resolution>(ResolveTicket_task(sample_ticket(), fake_resolution()));
  let usage = root.ai.aggregate_usage([first.meta, second.meta]);
  assert.equal(usage.input_tokens, 2)
  assert.equal(root.ai.estimated_cost(usage, root.ai.TokenPrice {
    input_per_million: 1.0,
    output_per_million: 2.0,
  }), 0.000006)
}

testset "integ-test-guide-06-usage-and-cost" {
  test "guide 06.03 live providers: aggregate exact counters and estimate explicit price" {
    let first = root.ai.drivers.generate_with_meta<Resolution>(
      ResolveTicket_task(sample_ticket(), live_openai()),
    );
    let second = root.ai.drivers.generate_with_meta<Resolution>(
      ResolveTicket_task(sample_ticket(), live_anthropic()),
    );
    let first_usage = first.meta.usage ?? root.ai.Usage {
      input_tokens: 0, output_tokens: 0, cached_input_tokens: 0,
      reasoning_tokens: 0, cost_usd: null,
    };
    let second_usage = second.meta.usage ?? root.ai.Usage {
      input_tokens: 0, output_tokens: 0, cached_input_tokens: 0,
      reasoning_tokens: 0, cost_usd: null,
    };
    let aggregate = root.ai.aggregate_usage([first.meta, second.meta]);
    let estimate = root.ai.estimated_cost(aggregate, root.ai.TokenPrice {
      input_per_million: 1.0,
      output_per_million: 2.0,
    });
    log.info(baml.json.stringify({
      "guide": "06.03",
      "responses": 2,
      "input_tokens": aggregate.input_tokens,
      "output_tokens": aggregate.output_tokens,
      "estimated_cost": estimate,
    }));
    assert.equal(
      aggregate.input_tokens,
      first_usage.input_tokens + second_usage.input_tokens,
    )
    assert.equal(
      aggregate.output_tokens,
      first_usage.output_tokens + second_usage.output_tokens,
    )
    assert.is_true(aggregate.input_tokens > 0)
    assert.is_true(aggregate.output_tokens > 0)
    assert.is_true(estimate > 0.0)
  }
}
