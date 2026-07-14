test "guide 02.01: one tool call is dispatched and the model finishes" {
  let provider = fake_tool_provider([one_knowledge_call()]);
  let outcome = root.ai.drivers.run_agent<Resolution>(
    ResolveTicketWithTools_task(sample_ticket(), provider, [knowledge_tool()]),
    guide_agent_options([knowledge_tool()], guide_dispatch),
  );
  match (outcome) {
    let done: root.ai.Done<Resolution> => assert.equal(done.value.category, "billing"),
    _ => assert.is_true(false),
  }
}

testset "integ-test-guide-02-one-tool" {
  test "guide 02.01 live OpenAI: tool call and final typed value" {
    let provider = root.ai.OpenAi {
      model: "gpt-5.6-luna",
      api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
      base_url: null,
      extra_headers: null,
      extra_body: null,
    };
    let outcome = root.ai.drivers.run_agent<Resolution>(
      ResolveTicketWithTools_task(sample_ticket(), provider, [knowledge_tool()]),
      guide_agent_options([knowledge_tool()], guide_dispatch),
    );
    match (outcome) {
      let done: root.ai.Done<Resolution> => assert.is_true(done.value.reply.length() > 0),
      _ => assert.is_true(false),
    }
  }
}
