test "guide 02.08: a handoff tool returns control to the application" {
  let handoff_tool = account_tool().as_handoff();
  let provider = fake_tool_provider([root.ai.ToolCall {
    id: "handoff-1", name: "lookup_account", args: { "customer_id": "C-1" },
  }]);
  let outcome = root.ai.drivers.run_agent<Resolution>(
    ResolveTicketWithTools_task(sample_ticket(), provider, [handoff_tool]),
    guide_agent_options([handoff_tool], guide_dispatch),
  );
  match (outcome) {
    let handoff: root.ai.Handoff => assert.equal(handoff.to, "lookup_account"),
    _ => assert.is_true(false),
  }
}

test "guide 02.08: a step budget returns resumable state" {
  let options = root.ai.AgentOptions {
    ...guide_agent_options([knowledge_tool()], guide_dispatch),
    budget: root.ai.Budget { max_steps: 1, max_cost_usd: null },
  };
  let outcome = root.ai.drivers.run_agent<Resolution>(
    ResolveTicketWithTools_task(sample_ticket(), fake_tool_provider([one_knowledge_call()]), [knowledge_tool()]),
    options,
  );
  match (outcome) {
    let stopped: root.ai.BudgetReached => {
      assert.equal(stopped.steps_taken, 1);
      assert.equal(stopped.reason, "max_steps");
    },
    _ => assert.is_true(false),
  }
}

test "guide 02.08: a cost budget stops between provider steps" {
  let provider = root.ai.ScriptedToolProvider {
    turns: [root.ai.ToolCalls { calls: [one_knowledge_call()] }],
    final_output: `{
      "category":"billing",
      "priority":"Urgent",
      "summary":"not reached",
      "reply":"not reached"
    }`,
    usage_per_step: root.ai.Usage {
      input_tokens: 10,
      output_tokens: 5,
      cached_input_tokens: 0,
      reasoning_tokens: 0,
      cost_usd: 0.02,
    },
  };
  let options = root.ai.AgentOptions {
    ...guide_agent_options([knowledge_tool()], guide_dispatch),
    budget: root.ai.Budget { max_steps: null, max_cost_usd: 0.01 },
  };
  let outcome = root.ai.drivers.run_agent<Resolution>(
    ResolveTicketWithTools_task(sample_ticket(), provider, [knowledge_tool()]),
    options,
  );
  match (outcome) {
    let stopped: root.ai.BudgetReached => {
      assert.equal(stopped.steps_taken, 1);
      assert.equal(stopped.reason, "max_cost_usd");
    },
    _ => assert.is_true(false),
  }
}
