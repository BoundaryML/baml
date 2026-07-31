test "guide 01.01: a direct typed call returns the declared class" {
  let resolution = ResolveTicket(sample_ticket());
  assert.equal(resolution.category, "billing")
  assert.equal(resolution.priority, TicketPriority.Urgent)
}

testset "integ-test-guide-01-direct-typed-call" {
  test "guide 01.01 live OpenAI: direct typed call" {
    let result = ResolveTicket@task(sample_ticket()).with_provider(root.openai.OpenAIProvider {
      ...root.openai.responses(),
      model: "gpt-5.6-luna",
      api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
    }).complete(runner = ai.run.Agent<Resolution>.new());
    assert.is_true(result.reply.length() > 0)
  }

  test "guide 01.01 live Anthropic: same task and return type" {
    let result = ResolveTicket@task(sample_ticket()).with_provider(root.anthropic.AnthropicProvider {
      ...root.anthropic.messages(),
      model: "claude-sonnet-4-6",
      api_key: baml.env.get_or_panic("ANTHROPIC_API_KEY"),
    }).complete(runner = ai.run.Agent<Resolution>.new());
    assert.is_true(result.summary.length() > 0)
  }
}
