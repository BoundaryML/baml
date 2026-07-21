test "guide 02.08: a handoff tool returns control to the application" {
  let handoff_tool = account_tool().as_handoff();
  let provider = fake_tool_provider([root.ai.ToolCall {
    id: "handoff-1", name: "lookup_account", args: { "customer_id": "C-1" },
  }]);
  let outcome = root.ai.drivers.run_agent<Resolution>(
    ResolveTicketWithTools_task(sample_ticket(), provider, [handoff_tool]),
    root.ai.AgentOptions.new([handoff_tool], guide_dispatch),
  );
  match (outcome) {
    let handoff: root.ai.Handoff => assert.equal(handoff.to, "lookup_account"),
    _ => assert.is_true(false),
  }
}

test "guide 02.08: a step budget returns resumable state" {
  let options = root.ai.AgentOptions.new(
    [knowledge_tool()],
    guide_dispatch,
    budget = root.ai.Budget { max_steps: 1, max_cost_usd: null },
  );
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
  let options = root.ai.AgentOptions.new(
    [knowledge_tool()],
    guide_dispatch,
    budget = root.ai.Budget { max_steps: null, max_cost_usd: 0.01 },
  );
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

class LiveCostedToolProvider {
  inner: root.ai.OpenAi,
  cost_per_step_usd: float,

  implements root.ai.Provider {
    function name(self) -> string throws never { "costed-" + self.inner.name() }
    function render_shorthand(self) -> string throws never { self.inner.render_shorthand() }
  }
}

implements root.ai.ToolCallingProvider for LiveCostedToolProvider {
  function begin<T>(self, task: root.ai.Task<T>) -> root.ai.Transcript
      throws baml.errors.ToolError | baml.errors.UnknownError {
    self.inner.begin<T>(task.with_provider(self.inner))
  }

  function step<T>(self, transcript: root.ai.Transcript, tools: root.ai.Tool[])
      -> root.ai.ModelStep<T> throws baml.errors.ToolError | baml.errors.UnknownError {
    let result = self.inner.step<T>(transcript, tools);
    match (result.meta.usage) {
      let usage: root.ai.Usage => { usage.cost_usd = self.cost_per_step_usd; },
      null => {},
    }
    result
  }

  function submit(
    self,
    transcript: root.ai.Transcript,
    results: root.ai.ToolResult[],
  ) -> root.ai.Transcript throws baml.errors.ToolError | baml.errors.UnknownError {
    self.inner.submit(transcript, results)
  }
}

testset "integ-test-guide-02-budgets" {
  test "guide 02.08 live OpenAI: step budget stops after one real provider turn" {
    let provider = live_openai();
    let options = root.ai.AgentOptions.new(
      [knowledge_tool()],
      guide_dispatch,
      budget = root.ai.Budget { max_steps: 1, max_cost_usd: null },
    );
    let outcome = root.ai.drivers.run_agent<Resolution>(
      ToolInstructions_task(
        provider,
        [knowledge_tool()],
        `Call search_knowledge with query "duplicate charge" before returning a final resolution.`,
      ),
      options,
    );
    match (outcome) {
      let stopped: root.ai.BudgetReached => {
        log.info("live step budget " + {
          "steps": stopped.steps_taken,
          "reason": stopped.reason,
        }.to_string());
        assert.equal(stopped.steps_taken, 1);
        assert.equal(stopped.reason, "max_steps");
      },
      _ => assert.is_true(false),
    }
  }

  test "guide 02.08 live OpenAI: cost budget stops after metered real provider usage" {
    let provider = LiveCostedToolProvider {
      inner: live_openai(),
      cost_per_step_usd: 0.02,
    };
    let options = root.ai.AgentOptions.new(
      [knowledge_tool()],
      guide_dispatch,
      budget = root.ai.Budget { max_steps: null, max_cost_usd: 0.01 },
    );
    let outcome = root.ai.drivers.run_agent<Resolution>(
      ToolInstructions_task(
        provider,
        [knowledge_tool()],
        `Call search_knowledge with query "duplicate charge" before returning a final resolution.`,
      ),
      options,
    );
    match (outcome) {
      let stopped: root.ai.BudgetReached => {
        log.info("live cost budget " + {
          "steps": stopped.steps_taken,
          "reason": stopped.reason,
        }.to_string());
        assert.equal(stopped.steps_taken, 1);
        assert.equal(stopped.reason, "max_cost_usd");
      },
      _ => assert.is_true(false),
    }
  }
}
