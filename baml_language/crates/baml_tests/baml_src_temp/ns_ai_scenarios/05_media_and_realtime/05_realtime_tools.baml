test "guide 05.05: realtime tasks retain the same tool roster" {
  let task = VoiceSupport_task("look up the account", root.ai.FakeRealtimeProvider { transcript: "done" });
  let with_tools = task.with_tools([account_tool()]);
  assert.equal(with_tools.tools[0].name, "lookup_account")
}

testset "integ-test-guide-05-realtime-tools-pending-adapter" {
  test "guide 05.05 live OpenAI: equivalent tool contract through the tool-call adapter" {
    let provider = root.ai.OpenAi {
      model: "gpt-5.6-luna",
      api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
      base_url: null, extra_headers: null, extra_body: null,
    };
    let outcome = root.ai.drivers.run_agent<Resolution>(
      ResolveTicketWithTools_task(sample_ticket(), provider, [account_tool()]),
      guide_agent_options([account_tool()], guide_dispatch),
    );
    match (outcome) {
      let done: root.ai.Done<Resolution> => assert.is_true(done.value.reply.length() > 0),
      _ => assert.is_true(false),
    }
  }
}
