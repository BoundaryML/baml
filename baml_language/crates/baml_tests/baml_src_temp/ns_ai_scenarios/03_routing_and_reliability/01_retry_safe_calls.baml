test "guide 03.01: retry repeats only a replay-safe direct call" {
  //# Configure one transient failure
  let flaky = root.ai.FakeProvider { ...fake_resolution(), failures_remaining: 1 };
  //# Execute through a bounded retry wrapper
  let value = ResolveTicket_manual(sample_ticket(), root.ai.retry(flaky, 2));
  //# Verify the value and attempt count
  assert.equal(value.category, "billing")
  assert.equal(flaky.calls, 2)
}

testset "integ-test-guide-03-retry" {
  test "guide 03.01 live OpenAI: retry wrapper preserves the typed result" {
    //# Wrap a live provider with bounded retry
    let provider = root.ai.retry(root.ai.OpenAi {
      ...root.ai.openai(),
      model: "gpt-5.6-luna",
      api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
    }, 2);
    //# Execute and verify the typed result
    assert.is_true(ResolveTicket_manual(sample_ticket(), provider).reply.length() > 0)
  }
}
