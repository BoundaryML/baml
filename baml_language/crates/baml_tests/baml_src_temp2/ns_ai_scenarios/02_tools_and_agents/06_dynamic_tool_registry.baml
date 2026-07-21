test "guide 02.06: a hook can add a tool before the next model step" {
  let registry = root.ai.ToolRegistry.new([knowledge_tool()]);
  let hooks = GuideHooks { ...no_op_hooks(), tool_to_add: account_tool() };
  let options = root.ai.AgentOptions.new(
    [],
    guide_dispatch,
    tool_registry = registry,
    hooks = hooks,
  );
  let task = ResolveTicketWithTools_task(
    sample_ticket(),
    fake_tool_provider([one_knowledge_call()]),
    [],
  );
  let outcome = task.drive(
    root.ai.driver.agent<Resolution>(options),
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

class AddAccountAfterFirstStepHooks {
  after_calls: string[],

  implements root.ai.AgentHooks {
    function prepare_step(self, context: root.ai.StepContext) -> root.ai.StepPlan throws never {
      let has_account = context.tools.filter((tool: root.ai.Tool) -> bool {
        tool.name == "lookup_account"
      }).length() > 0;
      if (context.step == 1 && !has_account) {
        let next_tools = context.tools.slice(0, context.tools.length());
        next_tools.push(account_tool());
        return root.ai.StepPlan { provider: null, tools: next_tools, stop: null };
      }
      root.ai.StepPlan { provider: null, tools: null, stop: null }
    }

    function after_tool_call(self, event: root.ai.AfterToolCall) -> null throws never {
      self.after_calls.push(event.call.name);
      null
    }
  }
}

testset "integ-test-guide-02-dynamic-tool-registry" {
  test "guide 02.06 live OpenAI: a tool added after turn one is used on turn two" {
    let registry = root.ai.ToolRegistry.new([knowledge_tool()]);
    let hooks = AddAccountAfterFirstStepHooks { after_calls: [] };
    let observer = GuideObserver { kinds: [] };
    let provider = live_openai();
    let task = ToolInstructions_task(
      provider,
      [],
      `First call search_knowledge with query "duplicate charge". A lookup_account tool will be added after that result. Then call lookup_account with customer_id "C-1" before returning the final resolution.`,
    );
    let outcome = task.drive(
      root.ai.driver.agent<Resolution>(root.ai.AgentOptions.new(
        [],
        guide_dispatch,
        tool_registry = registry,
        hooks = hooks,
        observers = [observer],
      )),
    );
    log.info("live dynamic registry " + {
      "calls": hooks.after_calls,
      "registry": registry.snapshot().map((tool: root.ai.Tool) -> string { tool.name }),
      "events": observer.kinds,
    }.to_string());
    assert.equal(hooks.after_calls, ["search_knowledge", "lookup_account"])
    assert.contains(observer.kinds.join(","), "tool_roster_changed")
    match (outcome) {
      let done: root.ai.Done<Resolution> => assert.is_true(done.value.reply.length() > 0),
      _ => assert.is_true(false),
    }
  }
}
