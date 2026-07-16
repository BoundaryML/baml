test "guide 02.06: a hook can add a tool before the next model step" {
  let registry = root.ai.ToolRegistry.new([knowledge_tool()]);
  let hooks = GuideHooks { ...no_op_hooks(), tool_to_add: account_tool() };
  let options = guide_agent_options(
    [],
    guide_dispatch,
    tool_registry = registry,
    hooks = hooks,
  );
  let outcome = root.ai.drivers.run_agent<Resolution>(
    ResolveTicketWithTools_task(sample_ticket(), fake_tool_provider([one_knowledge_call()]), []),
    options,
  );
  match (outcome) {
    let done: root.ai.Done<Resolution> => match (done.transcript) {
      let transcript: root.ai.FakeToolTranscript => assert.equal(transcript.last_tools.length(), 2),
      _ => assert.is_true(false),
    },
    _ => assert.is_true(false),
  }
}

test "guide 02.06: duplicate tool names fail instead of shadowing" {
  let registry = root.ai.ToolRegistry.new([knowledge_tool()]);
  let outcome = registry.add(knowledge_tool()) catch (e) {
    let collision: baml.errors.ToolError => "collision",
  };
  assert.equal(outcome, "collision")
  assert.equal(registry.snapshot().length(), 1)
}

test "guide 02.06: replacement is explicit" {
  let registry = root.ai.ToolRegistry.new([knowledge_tool()]);
  let replacement = root.ai.Tool.typed<KnowledgeLookupArgs>(
    "search_knowledge",
    "Replacement handler contract.",
  );
  assert.is_true(registry.replace(replacement))
  assert.equal(registry.snapshot()[0].description, "Replacement handler contract.")
}
