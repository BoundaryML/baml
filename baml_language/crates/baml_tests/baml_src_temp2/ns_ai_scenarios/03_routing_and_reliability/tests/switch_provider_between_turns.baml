// Tests for the switch_provider_between_turns scenario.
test "guide 03.04: prepare_step can switch providers through conversation import" {
  let first = fake_tool_provider([one_knowledge_call()]);
  let second = root.ai.testing.FakeToolProvider {
    ...fake_tool_provider([]),
    provider_name: "fake-tools-secondary",
  };
  let trace = guide_agent_trace();
  let task = ResolveTicketWithTools_task(sample_ticket(), first, [search_knowledge]);
  let outcome = task.run(
    runner = root.ai.run.Agent<Resolution>.new(
      tools = [search_knowledge],
      prepare_step = (context) -> {
        root.ai.tools.StepPlan {
          provider: if (context.step == 1) { second } else { null },
          tools: null,
          stop: null,
        }
      },
      on_event = (event) -> {
        trace.events.push(event.kind());
        null
      },
    ),
  );
  match (outcome) {
    let done: root.ai.Done<Resolution> => assert.equal(
      done.conversation.provider().name(),
      "fake-tools-secondary",
    ),
    _ => assert.is_true(false),
  }
  assert.contains(trace.events.join(","), "provider_changed")
}

test "guide 03.04: OpenAI Responses imports portable message text" {
  let provider = root.openai.OpenAIProvider { ...root.openai.responses(), api_key: "unused" };
  let calls: json[] = [{
    "id": "call-1",
    "type": "function",
    "function": { "name": "search_knowledge", "arguments": "{}" },
  }];
  let history = root.ai.MessageHistory { entries: [
    root.ai.ChatMessage {
      message_role: root.ai.MessageRole.Assistant,
      content: [root.ai.messages.ProviderDataPart {
        provider: "openai",
        data: { "tool_calls": calls },
      }],
      metadata: null,
    },
    root.ai.ChatMessage {
      message_role: root.ai.MessageRole.Tool,
      content: [root.ai.messages.ProviderDataPart {
        provider: "openai",
        data: { "tool_call_id": "call-1", "output": { "ok": true } },
      }],
      metadata: null,
    },
  ] };
  let imported = provider.import_messages<Resolution>(history);
  match (imported.conversation) {
    let state: root.openai.internal.Conversation => {
      assert.equal(state.input.length(), 2)
      assert.equal(baml.json.path_or(state.input[0], ".role", ""), "assistant")
      assert.equal(baml.json.path_or(state.input[1], ".role", ""), "user")
      assert.equal(state.previous_response_id, null)
    },
    _ => assert.is_true(false),
  }
}

class NamedOpenAiToolProvider {
  label: string,
  inner: root.openai.OpenAIProvider,

  implements root.ai.Provider {
    function name(self) -> string throws never { self.label }
    function render_shorthand(self) -> string throws never { self.inner.render_shorthand() }
  }
}

implements root.ai.AgentProvider for NamedOpenAiToolProvider {
  function begin<T>(self, task: root.ai.Task<T>) -> root.ai.Conversation
      throws root.ai.Failure | baml.errors.UnknownError {
    self.inner.begin<T>(task.with_provider(self.inner))
  }

  function step<T>(self, conversation: root.ai.Conversation, tools: root.ai.tools.Tool[])
      -> root.ai.ModelStep<T> throws root.ai.Failure | baml.errors.UnknownError {
    self.inner.step<T>(conversation, tools)
  }

  function submit(
    self,
    conversation: root.ai.Conversation,
    results: root.ai.tools.ToolResult[],
  ) -> root.ai.Conversation throws root.ai.Failure | baml.errors.UnknownError {
    self
      .inner
      .as<root.ai.AgentProvider>
      .submit(conversation, results)
  }
}

implements root.ai.ConversationImportProvider for NamedOpenAiToolProvider {
  function import_messages<T>(self, messages: root.ai.Messages)
      -> root.ai.ConversationImport throws baml.errors.UnknownError {
    self.inner.import_messages<T>(messages)
  }
}

testset "integ-test-guide-03-switch-between-turns" {
  test "guide 03.04 live OpenAI: a second provider imports conversation between turns" {
    let first = live_openai();
    let second = NamedOpenAiToolProvider {
      label: "openai-secondary",
      inner: live_openai(),
    };
    let trace = guide_agent_trace();
    let observer = GuideObserver { kinds: [] };
    let task = ToolInstructions_task(
      first,
      [search_knowledge],
      `Call search_knowledge with query "duplicate charge". After its result the run will move to another provider; that provider must return the final resolution from the imported conversation.`,
    );
    let outcome = task.run(
      runner = root.ai.run.Agent<Resolution>.new(
        tools = [search_knowledge],
        prepare_step = (context) -> {
          root.ai.tools.StepPlan {
            provider: if (context.step == 1) { second } else { null },
            tools: null,
            stop: null,
          }
        },
        on_event = (event) -> {
          trace.events.push(event.kind());
          null
        },
        observers = [observer],
      ),
    );
    log.info("live provider switch " + {
      "events": observer.kinds,
      "callback_events": trace.events,
    }.to_string());
    assert.contains(observer.kinds.join(","), "provider_changed")
    match (outcome) {
      let done: root.ai.Done<Resolution> => {
        assert.equal(done.metadata.provider, "openai-secondary");
        assert.is_true(done.value.reply.length() > 0);
      },
      _ => assert.is_true(false),
    }
  }
}
