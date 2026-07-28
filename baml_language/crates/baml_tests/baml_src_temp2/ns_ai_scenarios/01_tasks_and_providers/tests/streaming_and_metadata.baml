test "guide 01.03: a generation-only fake does not claim streaming" {
  let task = ResolveTicket_task(sample_ticket(), fake_resolution());
  let kind = match (task.provider) {
    let streaming: root.ai.StreamingProvider => "streaming",
    _ => "generation-only",
  };
  assert.equal(kind, "generation-only")
}

testset "integ-test-guide-01-streaming" {
  test "guide 01.03 live OpenAI: structured stream reaches a final value" {
    let value = drain_resolution_interface_stream(ResolveTicket_task(sample_ticket(), root.openai.Chat {
      ...root.openai.chat(),
      model: "gpt-5.6-luna",
      api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
    }));
    assert.is_true(value.reply.length() > 0)
  }

  test "guide 01.03 live Anthropic: same stream consumer" {
    let value = drain_resolution_interface_stream(ResolveTicket_task(sample_ticket(), root.anthropic.Messages {
      ...root.anthropic.messages(),
      model: "claude-sonnet-4-6",
      api_key: baml.env.get_or_panic("ANTHROPIC_API_KEY"),
    }));
    assert.is_true(value.summary.length() > 0)
  }
}
