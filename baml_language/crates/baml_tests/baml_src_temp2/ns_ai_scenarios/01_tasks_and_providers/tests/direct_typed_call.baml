test "guide 01.01: a direct typed call returns the declared class" {
  let resolution = ResolveTicket_manual(sample_ticket(), fake_resolution());
  assert.equal(resolution.category, "billing")
  assert.equal(resolution.priority, TicketPriority.Urgent)
}

testset "integ-test-guide-01-direct-typed-call" {
  test "guide 01.01 live OpenAI: direct typed call" {
    let result = ResolveTicket_manual(sample_ticket(), root.openai.Chat {
      ...root.openai.chat(),
      model: "gpt-5.6-luna",
      api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
    });
    assert.is_true(result.reply.length() > 0)
  }

  test "guide 01.01 live Anthropic: same task and return type" {
    let result = ResolveTicket_manual(sample_ticket(), root.anthropic.Messages {
      ...root.anthropic.messages(),
      model: "claude-sonnet-4-6",
      api_key: baml.env.get_or_panic("ANTHROPIC_API_KEY"),
    });
    assert.is_true(result.summary.length() > 0)
  }
}
