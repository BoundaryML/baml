class ToolRosterHooks {
  apply_at_step: int,
  remove_name: string?,
  clear_roster: bool,
  application_tools: root.ai.Tool[]?,
  next_provider: root.ai.Provider?,

  implements root.ai.AgentHooks {
    function prepare_step(self, context: root.ai.StepContext) -> root.ai.StepPlan throws never {
      if (context.step != self.apply_at_step) {
        return root.ai.StepPlan { provider: null, tools: null, stop: null };
      }
      let next_tools: root.ai.Tool[]? = match (self.application_tools) {
        let replacement: root.ai.Tool[] => replacement,
        null if self.clear_roster => [],
        null => match (self.remove_name) {
          let name: string => context.tools.filter((tool: root.ai.Tool) -> bool {
            tool.name != name
          }),
          null => null,
        },
      };
      root.ai.StepPlan {
        provider: self.next_provider,
        tools: next_tools,
        stop: null,
      }
    }
  }
}

function tool_roster_hooks() -> ToolRosterHooks throws never {
  ToolRosterHooks {
    apply_at_step: 1,
    remove_name: null,
    clear_roster: false,
    application_tools: null,
    next_provider: null,
  }
}

test "guide 02.09: remove one application tool between provider steps" {
  let registry = root.ai.ToolRegistry.new([knowledge_tool(), account_tool()]);
  let hooks = ToolRosterHooks { ...tool_roster_hooks(), remove_name: "lookup_account" };
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
      let transcript: root.ai.FakeToolTranscript => {
        assert.equal(transcript.last_tools.length(), 1);
        assert.equal(transcript.last_tools[0].name, "search_knowledge");
      },
      _ => assert.is_true(false),
    },
    _ => assert.is_true(false),
  }
}

test "guide 02.09: an explicit empty list removes all application tools" {
  let hooks = ToolRosterHooks { ...tool_roster_hooks(), application_tools: [] };
  let options = root.ai.AgentOptions.new(
    [knowledge_tool(), account_tool()],
    guide_dispatch,
    hooks = hooks,
  );
  let task = ResolveTicketWithTools_task(
    sample_ticket(),
    fake_tool_provider([one_knowledge_call()]),
    [knowledge_tool(), account_tool()],
  );
  let outcome = task.drive(
    root.ai.driver.agent<Resolution>(options),
  );
  match (outcome) {
    let done: root.ai.Done<Resolution> => match (done.transcript) {
      let transcript: root.ai.FakeToolTranscript => assert.equal(transcript.last_tools.length(), 0),
      _ => assert.is_true(false),
    },
    _ => assert.is_true(false),
  }
}

test "guide 02.09: an empty replacement updates the registry and persists" {
  let registry = root.ai.ToolRegistry.new([knowledge_tool(), account_tool()]);
  let hooks = ToolRosterHooks { ...tool_roster_hooks(), clear_roster: true };
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
      let transcript: root.ai.FakeToolTranscript => {
        assert.equal(registry.snapshot().length(), 0);
        assert.equal(transcript.last_tools.length(), 0);
      },
      _ => assert.is_true(false),
    },
    _ => assert.is_true(false),
  }
}

test "guide 02.09: remove application and provider-owned tools together" {
  let web_search = root.ai.ProviderTool {
    name: "web_search",
    kind: "openai.web_search",
    config: { "search_context_size": "low" },
  };
  let initial = fake_tool_provider_with_builtins([one_knowledge_call()], [web_search]);
  // Same provider display name and model behavior, different configuration.
  // A non-null StepPlan.provider must therefore switch even though name() is equal.
  let without_builtins = fake_tool_provider_with_builtins([], []);
  let hooks = ToolRosterHooks {
    ...tool_roster_hooks(),
    application_tools: [],
    next_provider: without_builtins,
  };
  let options = root.ai.AgentOptions.new([knowledge_tool()], guide_dispatch, hooks = hooks);
  let task = ResolveTicketWithTools_task(sample_ticket(), initial, [knowledge_tool()]);
  let outcome = task.drive(
    root.ai.driver.agent<Resolution>(options),
  );
  match (outcome) {
    let done: root.ai.Done<Resolution> => match (done.transcript) {
      let transcript: root.ai.FakeToolTranscript => {
        assert.equal(transcript.owner.name(), "fake-tools");
        assert.equal(transcript.last_tools.length(), 0);
        assert.equal(transcript.last_provider_tools.length(), 0);
      },
      _ => assert.is_true(false),
    },
    _ => assert.is_true(false),
  }
}

testset "integ-test-guide-02-remove-tools" {
  test "guide 02.09 live OpenAI: removing a tool changes the next provider request" {
    let registry = root.ai.ToolRegistry.new([knowledge_tool(), account_tool()]);
    let hooks = ToolRosterHooks { ...tool_roster_hooks(), remove_name: "lookup_account" };
    let observer = GuideObserver { kinds: [] };
    let provider = live_openai();
    let task = ToolInstructions_task(
      provider,
      [],
      `First call search_knowledge with query "duplicate charge". After its result, lookup_account will be removed. Return the final resolution without calling a removed tool.`,
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
    match (outcome) {
      let done: root.ai.Done<Resolution> => match (done.transcript) {
        let transcript: root.ai.OpenAiToolTranscript => {
          let names = transcript.tools_json.map((schema: json) -> string {
            baml.json.path_or<string>(schema, ".function.name", "")
          });
          log.info("live removed tools " + {
            "registry": registry.snapshot().map((tool: root.ai.Tool) -> string { tool.name }),
            "provider_tools": names,
            "events": observer.kinds,
          }.to_string());
          assert.equal(names, ["search_knowledge"]);
          assert.equal(registry.snapshot().length(), 1);
          assert.contains(observer.kinds.join(","), "tool_roster_changed");
        },
        _ => assert.is_true(false),
      },
      _ => assert.is_true(false),
    }
  }
}
