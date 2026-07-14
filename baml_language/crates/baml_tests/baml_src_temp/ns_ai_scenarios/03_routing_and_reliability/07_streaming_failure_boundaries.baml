test "guide 03.07: a generation retry wrapper does not invent stream replay" {
  let provider: root.ai.Provider = root.ai.retry(fake_resolution(), 2);
  let capability = match (provider) {
    let streaming: root.ai.StreamingProvider => "streaming",
    _ => "not-streaming",
  };
  assert.equal(capability, "not-streaming")
}

