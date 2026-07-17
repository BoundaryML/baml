test "guide 03.04: prepare_step can switch providers through transcript import" {
  let first = fake_tool_provider([one_knowledge_call()]);
  let second = root.ai.FakeToolProvider {
    ...fake_tool_provider([]),
    provider_name: "fake-tools-secondary",
  };
  let hooks = GuideHooks { ...no_op_hooks(), switch_after_step: second };
  let options = guide_agent_options([knowledge_tool()], guide_dispatch, hooks = hooks);
  let outcome = root.ai.drivers.run_agent<Resolution>(
    ResolveTicketWithTools_task(sample_ticket(), first, [knowledge_tool()]),
    options,
  );
  match (outcome) {
    let done: root.ai.Done<Resolution> => assert.equal(
      done.transcript.provider().name(),
      "fake-tools-secondary",
    ),
    _ => assert.is_true(false),
  }
  assert.contains(hooks.hook_events.join(","), "provider_changed")
}

class NamedOpenAiToolProvider {
  label: string,
  inner: root.ai.OpenAi,

  implements root.ai.Provider {
    function name(self) -> string throws never { self.label }
    function render_shorthand(self) -> string throws never { self.inner.render_shorthand() }
  }
}

implements root.ai.ToolCallingProvider for NamedOpenAiToolProvider {
  function begin<T>(self, task: root.ai.Task<T>) -> root.ai.Transcript
      throws baml.errors.ToolError | baml.errors.UnknownError {
    self.inner.begin<T>(task.with_provider(self.inner))
  }

  function step<T>(self, transcript: root.ai.Transcript, tools: root.ai.Tool[])
      -> root.ai.ModelStep<T> throws baml.errors.ToolError | baml.errors.UnknownError {
    self.inner.step<T>(transcript, tools)
  }

  function submit(
    self,
    transcript: root.ai.Transcript,
    results: root.ai.ToolResult[],
  ) -> root.ai.Transcript throws baml.errors.ToolError | baml.errors.UnknownError {
    self.inner.submit(transcript, results)
  }
}

implements root.ai.TranscriptImportProvider for NamedOpenAiToolProvider {
  function import_conversation(self, conversation: root.ai.Conversation)
      -> root.ai.TranscriptImport throws baml.errors.UnknownError {
    self.inner.import_conversation(conversation)
  }
}

testset "integ-test-guide-03-switch-between-turns" {
  test "guide 03.04 live OpenAI: a second provider imports conversation between turns" {
    let first = live_openai();
    let second = NamedOpenAiToolProvider {
      label: "openai-secondary",
      inner: live_openai(),
    };
    let hooks = GuideHooks { ...no_op_hooks(), switch_after_step: second };
    let observer = GuideObserver { kinds: [] };
    let outcome = root.ai.drivers.run_agent<Resolution>(
      ToolInstructions_task(
        first,
        [knowledge_tool()],
        `Call search_knowledge with query "duplicate charge". After its result the run will move to another provider; that provider must return the final resolution from the imported conversation.`,
      ),
      guide_agent_options(
        [knowledge_tool()],
        guide_dispatch,
        hooks = hooks,
        observers = [observer],
      ),
    );
    log.info("live provider switch " + {
      "events": observer.kinds,
      "hook_events": hooks.hook_events,
    }.to_string());
    assert.contains(observer.kinds.join(","), "provider_changed")
    match (outcome) {
      let done: root.ai.Done<Resolution> => {
        assert.equal(done.meta.provider, "openai-secondary");
        assert.is_true(done.value.reply.length() > 0);
      },
      _ => assert.is_true(false),
    }
  }
}
