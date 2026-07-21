test "guide 01.03: a generation-only fake does not claim streaming" {
  let task = ResolveTicket_task(sample_ticket(), fake_resolution());
  let kind = match (task.provider) {
    let streaming: root.ai.StreamingProvider => "streaming",
    _ => "generation-only",
  };
  assert.equal(kind, "generation-only")
}

function drain_resolution_interface_stream(
  task: root.ai.Task<Resolution>,
) -> Resolution {
  // The prototype spells both projected types explicitly until constructor
  // inference can use Task.drive's expected Driver input.
  // Its throws annotation is also inferred because associated Error is not
  // yet normalized when validating an explicit enclosing throws union.
  let stream = task.drive(
    root.ai.driver.stream<Resolution$stream, Resolution>(),
  );
  stream.final()
}

testset "integ-test-guide-01-streaming" {
  test "guide 01.03 live OpenAI: structured stream reaches a final value" {
    let value = drain_resolution_interface_stream(ResolveTicket_task(sample_ticket(), root.ai.OpenAi {
      ...root.ai.openai(),
      model: "gpt-5.6-luna",
      api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
    }));
    assert.is_true(value.reply.length() > 0)
  }

  test "guide 01.03 live Anthropic: same stream consumer" {
    let value = drain_resolution_interface_stream(ResolveTicket_task(sample_ticket(), root.ai.Anthropic {
      ...root.ai.anthropic(),
      model: "claude-sonnet-4-6",
      api_key: baml.env.get_or_panic("ANTHROPIC_API_KEY"),
    }));
    assert.is_true(value.summary.length() > 0)
  }
}
