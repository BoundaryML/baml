test "guide 06.04: one task can be tested against multiple provider implementations" {
  //# Configure equivalent provider candidates
  let first = root.ai.FakeProvider { ...fake_resolution(), provider_name: "candidate-a" };
  let second = root.ai.FakeProvider { ...fake_resolution(), provider_name: "candidate-b" };
  //# Run the same task against every candidate
  for (let provider in [first, second]) {
    //## Verify one provider result
    assert.equal(ResolveTicket_manual(sample_ticket(), provider).category, "billing")
  }
}

testset "integ-test-guide-06-provider-matrix" {
  test "guide 06.04 live OpenAI" {
    //# Run the task against live OpenAI
    let result = ResolveTicket_manual(sample_ticket(), root.ai.OpenAi {
      ...root.ai.openai(),
      model: "gpt-5.6-luna",
      api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
    });
    //# Verify the live result
    assert.is_true(result.summary.length() > 0)
  }

  test "guide 06.04 live Anthropic" {
    //# Run the task against live Anthropic
    let result = ResolveTicket_manual(sample_ticket(), root.ai.Anthropic {
      ...root.ai.anthropic(),
      model: "claude-sonnet-4-6",
      api_key: baml.env.get_or_panic("ANTHROPIC_API_KEY"),
    });
    //# Verify the live result
    assert.is_true(result.summary.length() > 0)
  }
}
