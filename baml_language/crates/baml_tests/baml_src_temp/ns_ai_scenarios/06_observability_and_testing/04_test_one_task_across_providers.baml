test "guide 06.04: one task can be tested against multiple provider implementations" {
  let first = root.ai.FakeProvider { ...fake_resolution(), provider_name: "candidate-a" };
  let second = root.ai.FakeProvider { ...fake_resolution(), provider_name: "candidate-b" };
  for (let provider in [first, second]) {
    assert.equal(ResolveTicket_manual(sample_ticket(), provider).category, "billing")
  }
}

testset "integ-test-guide-06-provider-matrix" {
  test "guide 06.04 live OpenAI" {
    let result = ResolveTicket_manual(sample_ticket(), root.ai.OpenAi {
      ...root.ai.openai(),
      model: "gpt-5.6-luna",
      api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
    });
    assert.is_true(result.summary.length() > 0)
  }

  test "guide 06.04 live Anthropic" {
    let result = ResolveTicket_manual(sample_ticket(), root.ai.Anthropic {
      ...root.ai.anthropic(),
      model: "claude-sonnet-4-6",
      api_key: baml.env.get_or_panic("ANTHROPIC_API_KEY"),
    });
    assert.is_true(result.summary.length() > 0)
  }
}
