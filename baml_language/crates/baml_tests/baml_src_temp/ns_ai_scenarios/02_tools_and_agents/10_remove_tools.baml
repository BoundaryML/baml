class ToolRosterHooks {
  apply_at_step: int,
  registry: root.ai.ToolRegistry?,
  remove_name: string?,
  clear_registry: bool,
  application_tools: root.ai.Tool[]?,
  next_provider: root.ai.Provider?,

  implements root.ai.AgentHooks {
    function prepare_step(self, context: root.ai.StepContext) -> root.ai.StepPlan throws never {
      if (context.step != self.apply_at_step) {
        return root.ai.StepPlan { provider: null, tools: null, stop: null };
      }
      match (self.registry) {
        let registry: root.ai.ToolRegistry => {
          if (self.clear_registry) {
            registry.clear();
          } else {
            match (self.remove_name) {
              let name: string => { let _ = registry.remove(name); },
              null => {},
            }
          }
        },
        null => {},
      }
      root.ai.StepPlan {
        provider: self.next_provider,
        tools: self.application_tools,
        stop: null,
      }
    }
  }
}

function tool_roster_hooks() -> ToolRosterHooks throws never {
  ToolRosterHooks {
    apply_at_step: 1,
    registry: null,
    remove_name: null,
    clear_registry: false,
    application_tools: null,
    next_provider: null,
  }
}

test "guide 02.09: remove one application tool between provider steps" {
  let registry = root.ai.ToolRegistry.new([knowledge_tool(), account_tool()]);
  let hooks = tool_roster_hooks();
  hooks.registry = registry;
  hooks.remove_name = "lookup_account";
  let options = guide_agent_options([], guide_dispatch);
  options.tool_registry = registry;
  options.hooks = hooks;
  let outcome = root.ai.drivers.run_agent<Resolution>(
    ResolveTicketWithTools_task(sample_ticket(), fake_tool_provider([one_knowledge_call()]), []),
    options,
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
  let hooks = tool_roster_hooks();
  hooks.application_tools = [];
  let options = guide_agent_options([knowledge_tool(), account_tool()], guide_dispatch);
  options.hooks = hooks;
  let outcome = root.ai.drivers.run_agent<Resolution>(
    ResolveTicketWithTools_task(
      sample_ticket(),
      fake_tool_provider([one_knowledge_call()]),
      [knowledge_tool(), account_tool()],
    ),
    options,
  );
  match (outcome) {
    let done: root.ai.Done<Resolution> => match (done.transcript) {
      let transcript: root.ai.FakeToolTranscript => assert.equal(transcript.last_tools.length(), 0),
      _ => assert.is_true(false),
    },
    _ => assert.is_true(false),
  }
}

test "guide 02.09: clearing a registry persists across later steps" {
  let registry = root.ai.ToolRegistry.new([knowledge_tool(), account_tool()]);
  let hooks = tool_roster_hooks();
  hooks.registry = registry;
  hooks.clear_registry = true;
  let options = guide_agent_options([], guide_dispatch);
  options.tool_registry = registry;
  options.hooks = hooks;
  let outcome = root.ai.drivers.run_agent<Resolution>(
    ResolveTicketWithTools_task(sample_ticket(), fake_tool_provider([one_knowledge_call()]), []),
    options,
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
  let hooks = tool_roster_hooks();
  hooks.application_tools = [];
  hooks.next_provider = without_builtins;
  let options = guide_agent_options([knowledge_tool()], guide_dispatch);
  options.hooks = hooks;
  let outcome = root.ai.drivers.run_agent<Resolution>(
    ResolveTicketWithTools_task(sample_ticket(), initial, [knowledge_tool()]),
    options,
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
