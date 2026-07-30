test "guide 01.03: an Agent-only fake does not claim streaming" {
  let task = ResolveTicket@task(sample_ticket()).with_provider(fake_resolution());
  let kind = match (task.provider) {
    let streaming: ai.StreamingProvider => "streaming",
    _ => "agent-only",
  };
  assert.equal(kind, "agent-only")
}

testset "integ-test-guide-01-streaming" {
  test "guide 01.03 live Anthropic: same stream consumer" {
    let value = drain_resolution_interface_stream(ResolveTicket@task(sample_ticket()).with_provider(root.anthropic.AnthropicProvider {
      ...root.anthropic.messages(),
      model: "claude-sonnet-4-6",
      api_key: baml.env.get_or_panic("ANTHROPIC_API_KEY"),
    }));
    assert.is_true(value.summary.length() > 0)
  }
}
