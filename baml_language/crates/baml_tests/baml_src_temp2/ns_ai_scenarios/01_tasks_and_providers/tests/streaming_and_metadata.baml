test "guide 01.03: an Agent-only fake does not claim streaming" {
  let task = ResolveTicket_task(sample_ticket(), fake_resolution());
  let kind = match (task.provider) {
    let streaming: root.ai.StreamingProvider => "streaming",
    _ => "agent-only",
  };
  assert.equal(kind, "agent-only")
}

testset "integ-test-guide-01-streaming" {
  test "guide 01.03 live Anthropic: same stream consumer" {
    let value = drain_resolution_interface_stream(ResolveTicket_task(sample_ticket(), root.anthropic.Messages {
      ...root.anthropic.messages(),
      model: "claude-sonnet-4-6",
      api_key: baml.env.get_or_panic("ANTHROPIC_API_KEY"),
    }));
    assert.is_true(value.summary.length() > 0)
  }
}
