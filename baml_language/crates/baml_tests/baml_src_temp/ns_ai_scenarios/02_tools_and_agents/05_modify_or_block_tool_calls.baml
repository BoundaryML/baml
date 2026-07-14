test "guide 02.05: before-tool hooks may rewrite a call" {
  let hooks = no_op_hooks();
  hooks.replace_name = "lookup_account";
  let options = guide_agent_options([knowledge_tool(), account_tool()], guide_dispatch);
  options.hooks = hooks;
  root.ai.drivers.run_agent<Resolution>(
    ResolveTicketWithTools_task(sample_ticket(), fake_tool_provider([one_knowledge_call()]), [knowledge_tool(), account_tool()]),
    options,
  );
  assert.equal(hooks.after_calls, ["lookup_account"])
}

test "guide 02.05: before-tool hooks may block a call" {
  let hooks = no_op_hooks();
  hooks.block_name = "search_knowledge";
  let options = guide_agent_options([knowledge_tool()], guide_dispatch);
  options.hooks = hooks;
  root.ai.drivers.run_agent<Resolution>(
    ResolveTicketWithTools_task(sample_ticket(), fake_tool_provider([one_knowledge_call()]), [knowledge_tool()]),
    options,
  );
  assert.equal(hooks.after_calls.length(), 0)
}
