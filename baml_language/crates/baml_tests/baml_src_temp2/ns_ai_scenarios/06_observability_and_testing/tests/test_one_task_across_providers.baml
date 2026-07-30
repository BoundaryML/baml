
test "guide 06.04: one task can be tested against multiple provider implementations" {
  //# Configure equivalent provider candidates
  let first = ai.testing.FakeProvider { ...fake_resolution(), provider_name: "candidate-a" };
  let second = ai.testing.FakeProvider { ...fake_resolution(), provider_name: "candidate-b" };
  //# Run the same task against every candidate
  for (let provider in [first, second]) {
    //## Verify one provider result
    assert.equal(ai.internal._require_done(ResolveTicket@task(sample_ticket()).with_provider(provider).run(runner = ai.run.Agent<Resolution>.new())).value.category, "billing")
  }
}

testset "integ-test-guide-06-provider-matrix" {
  test "guide 06.04 live OpenAI" {
    //# Run the task against live OpenAI
    let result = ai.internal._require_done(ResolveTicket@task(sample_ticket()).with_provider(root.openai.OpenAIProvider {
      ...root.openai.responses(),
      model: "gpt-5.6-luna",
      api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
    }).run(runner = ai.run.Agent<Resolution>.new())).value;
    //# Verify the live result
    log.info(baml.json.stringify({ "guide": "06.04", "provider": "openai", "summary_length": result.summary.length() }));
    assert.is_true(result.summary.length() > 0)
  }

  test "guide 06.04 live Anthropic" {
    //# Run the task against live Anthropic
    let result = ai.internal._require_done(ResolveTicket@task(sample_ticket()).with_provider(root.anthropic.AnthropicProvider {
      ...root.anthropic.messages(),
      model: "claude-sonnet-4-6",
      api_key: baml.env.get_or_panic("ANTHROPIC_API_KEY"),
    }).run(runner = ai.run.Agent<Resolution>.new())).value;
    //# Verify the live result
    log.info(baml.json.stringify({ "guide": "06.04", "provider": "anthropic", "summary_length": result.summary.length() }));
    assert.is_true(result.summary.length() > 0)
  }
}
