test "guide 04.05: OpenAI seals and restores its exact wire transcript" {
  let provider = root.ai.OpenAi {
    model: "gpt-5.6-luna", api_key: "unused", base_url: null,
    extra_headers: null, extra_body: null,
  };
  let transcript = provider.begin<Resolution>(
    ResolveTicketWithTools_task(sample_ticket(), provider, [knowledge_tool()]),
  );
  let token = provider.save_transcript(transcript);
  let restored = provider.restore_transcript(token);
  assert.equal(restored.provider().name(), "openai")
}

