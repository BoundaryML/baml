test "guide 01.04: changing providers re-renders the same task recipe" {
  let fast = root.ai.FakeProvider {
    ...root.ai.fake_output_provider(`{
      "category":"billing","priority":"Normal","summary":"fast","reply":"fast reply"
    }`),
    provider_name: "fast",
  };
  let careful = root.ai.FakeProvider {
    ...fake_resolution(),
    provider_name: "careful",
  };
  let first = ResolveTicket_task(sample_ticket(), fast);
  let second = first.with_provider(careful);
  assert.equal(first.provider_name(), "fast")
  assert.equal(second.provider_name(), "careful")
  assert.contains(baml.llm.prompt_to_text(second.prompt), "Charged twice")
}
