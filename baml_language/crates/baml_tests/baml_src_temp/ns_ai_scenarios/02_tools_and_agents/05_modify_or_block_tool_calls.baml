test "guide 02.05: before-tool hooks may rewrite a call" {
  let hooks = GuideHooks { ...no_op_hooks(), replace_name: "lookup_account" };
  let options = guide_agent_options(
    [knowledge_tool(), account_tool()],
    guide_dispatch,
    hooks = hooks,
  );
  root.ai.drivers.run_agent<Resolution>(
    ResolveTicketWithTools_task(sample_ticket(), fake_tool_provider([one_knowledge_call()]), [knowledge_tool(), account_tool()]),
    options,
  );
  assert.equal(hooks.after_calls, ["lookup_account"])
}

test "guide 02.05: before-tool hooks may block a call" {
  let hooks = GuideHooks { ...no_op_hooks(), block_name: "search_knowledge" };
  let options = guide_agent_options([knowledge_tool()], guide_dispatch, hooks = hooks);
  root.ai.drivers.run_agent<Resolution>(
    ResolveTicketWithTools_task(sample_ticket(), fake_tool_provider([one_knowledge_call()]), [knowledge_tool()]),
    options,
  );
  assert.equal(hooks.after_calls.length(), 0)
}

function account_alias_tool() -> root.ai.Tool throws never {
  root.ai.Tool.typed<AccountLookupArgs>(
    "search_knowledge",
    "Legacy account lookup alias. Call with customer_id.",
  )
}

testset "integ-test-guide-02-modify-or-block-tools" {
  test "guide 02.05 live OpenAI: a hook rewrites a real tool call before dispatch" {
    let hooks = GuideHooks { ...no_op_hooks(), replace_name: "lookup_account" };
    let provider = live_openai();
    let outcome = root.ai.drivers.run_agent<Resolution>(
      ToolInstructions_task(
        provider,
        [account_alias_tool(), account_tool()],
        `Call search_knowledge exactly once with customer_id "C-1". After its result, return the final resolution.`,
      ),
      guide_agent_options(
        [account_alias_tool(), account_tool()],
        guide_dispatch,
        hooks = hooks,
      ),
    );
    log.info("live rewritten tool call " + { "after_calls": hooks.after_calls }.to_string());
    assert.equal(hooks.after_calls, ["lookup_account"])
    match (outcome) {
      let done: root.ai.Done<Resolution> => assert.is_true(done.value.reply.length() > 0),
      _ => assert.is_true(false),
    }
  }

  test "guide 02.05 live OpenAI: a hook blocks a real tool call and returns an error result" {
    let hooks = GuideHooks { ...no_op_hooks(), block_name: "search_knowledge" };
    let observer = GuideObserver { kinds: [] };
    let provider = live_openai();
    let outcome = root.ai.drivers.run_agent<Resolution>(
      ToolInstructions_task(
        provider,
        [knowledge_tool()],
        `Call search_knowledge once with query "duplicate charge". If policy blocks it, do not retry the tool; return a final resolution explaining that the lookup was unavailable.`,
      ),
      guide_agent_options(
        [knowledge_tool()],
        guide_dispatch,
        hooks = hooks,
        observers = [observer],
      ),
    );
    log.info("live blocked tool call " + {
      "after_calls": hooks.after_calls,
      "events": observer.kinds,
    }.to_string());
    assert.equal(hooks.after_calls.length(), 0)
    assert.contains(observer.kinds.join(","), "tool_call_blocked")
    match (outcome) {
      let done: root.ai.Done<Resolution> => assert.is_true(done.value.reply.length() > 0),
      _ => assert.is_true(false),
    }
  }
}
