test "guide 02.03: one model step may dispatch multiple tool calls" {
  let calls = [
    one_knowledge_call(),
    root.ai.ToolCall {
      id: "call-2", name: "lookup_account", args: { "customer_id": "C-1" },
    },
  ];
  let provider = fake_tool_provider(calls);
  let outcome = root.ai.drivers.run_agent<Resolution>(
    ResolveTicketWithTools_task(sample_ticket(), provider, [knowledge_tool(), account_tool()]),
    root.ai.AgentOptions.new([knowledge_tool(), account_tool()], guide_dispatch),
  );
  match (outcome) {
    let done: root.ai.Done<Resolution> => assert.equal(done.value.priority, TicketPriority.Urgent),
    _ => assert.is_true(false),
  }
}

testset "integ-test-guide-02-parallel-tools" {
  test "guide 02.03 live OpenAI: two tools are requested in one provider turn" {
    let hooks = no_op_hooks();
    let provider = live_openai();
    let outcome = root.ai.drivers.run_agent<Resolution>(
      ToolInstructions_task(
        provider,
        [knowledge_tool(), account_tool()],
        `Resolve ticket T-100. In your first response you must call both search_knowledge with query "duplicate charge" and lookup_account with customer_id "C-1" as parallel tool calls. Wait for both results before returning the final value.`,
      ),
      root.ai.AgentOptions.new(
        [knowledge_tool(), account_tool()],
        guide_dispatch,
        hooks = hooks,
      ),
    );
    match (outcome) {
      let done: root.ai.Done<Resolution> => match (done.transcript) {
        let transcript: root.ai.OpenAiToolTranscript => {
          let parallel_turns = transcript.wire_messages.filter((message: json) -> bool {
            baml.json.path_or<string>(message, ".role", "") == "assistant"
              && baml.json.path_or<json[]>(message, ".tool_calls", []).length() == 2
          });
          log.info("live parallel tools " + {
            "dispatched": hooks.after_calls,
            "parallel_turns": parallel_turns.length(),
          }.to_string());
          assert.equal(parallel_turns.length(), 1);
          assert.equal(hooks.after_calls.length(), 2);
          assert.contains(hooks.after_calls.join(","), "search_knowledge");
          assert.contains(hooks.after_calls.join(","), "lookup_account");
        },
        _ => assert.is_true(false),
      },
      _ => assert.is_true(false),
    }
  }
}
