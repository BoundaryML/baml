test "guide 03.01: retry repeats only a replay-safe direct call" {
  let flaky = root.ai.FakeProvider { ...fake_resolution(), failures_remaining: 1 };
  let value = ResolveTicket_manual(sample_ticket(), root.ai.retry(flaky, 2));
  assert.equal(value.category, "billing")
  assert.equal(flaky.calls, 2)
}

testset "integ-test-guide-03-retry" {
  test "guide 03.01 live OpenAI: retry wrapper preserves the typed result" {
    let provider = root.ai.retry(root.ai.OpenAi {
      ...root.ai.openai(),
      model: "gpt-5.6-luna",
      api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
    }, 2);
    assert.is_true(ResolveTicket_manual(sample_ticket(), provider).reply.length() > 0)
  }
}
