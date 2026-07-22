test "guide 03.04: prepare_step can switch providers through conversation import" {
  let first = fake_tool_provider([one_knowledge_call()]);
  let second = root.ai.FakeToolProvider {
    ...fake_tool_provider([]),
    provider_name: "fake-tools-secondary",
  };
  let hooks = GuideHooks { ...no_op_hooks(), switch_after_step: second };
  let task = ResolveTicketWithTools_task(sample_ticket(), first, [knowledge_tool()]);
  let outcome = task.run(
    runner = root.ai.run.Agent<Resolution>.new(
      tools = [knowledge_tool()],
      hooks = hooks,
    ),
  );
  match (outcome) {
    let done: root.ai.Done<Resolution> => assert.equal(
      done.conversation.provider().name(),
      "fake-tools-secondary",
    ),
    _ => assert.is_true(false),
  }
  assert.contains(hooks.hook_events.join(","), "provider_changed")
}

test "guide 03.04: OpenAI import keeps a tool result paired with its tool call" {
  let provider = root.ai.OpenAi { ...root.ai.openai(), api_key: "unused" };
  let calls: json[] = [{
    "id": "call-1",
    "type": "function",
    "function": { "name": "search_knowledge", "arguments": "{}" },
  }];
  let history = root.ai.MessageHistory { entries: [
    root.ai.ChatMessage {
      message_role: root.ai.MessageRole.Assistant,
      content: [root.ai.ProviderDataPart {
        provider: "openai",
        data: { "tool_calls": calls },
      }],
      metadata: null,
    },
    root.ai.ChatMessage {
      message_role: root.ai.MessageRole.Tool,
      content: [root.ai.ProviderDataPart {
        provider: "openai",
        data: { "tool_call_id": "call-1", "output": { "ok": true } },
      }],
      metadata: null,
    },
  ] };
  let imported = provider.import_messages(history);
  match (imported.conversation) {
    let state: root.ai.OpenAiConversation => {
      assert.equal(state.wire_messages.length(), 2)
      assert.equal(baml.json.path_or(state.wire_messages[0], ".role", ""), "assistant")
      assert.equal(baml.json.path_or(state.wire_messages[0], ".tool_calls[0].id", ""), "call-1")
      assert.equal(baml.json.path_or(state.wire_messages[1], ".role", ""), "tool")
      assert.equal(baml.json.path_or(state.wire_messages[1], ".tool_call_id", ""), "call-1")
    },
    _ => assert.is_true(false),
  }
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
  function begin<T>(self, task: root.ai.Task<T>) -> root.ai.Conversation
      throws baml.errors.ToolError | baml.errors.UnknownError {
    self.inner.begin<T>(task.with_provider(self.inner))
  }

  function step<T>(self, conversation: root.ai.Conversation, tools: root.ai.Tool[])
      -> root.ai.ModelStep<T> throws baml.errors.ToolError | baml.errors.UnknownError {
    self.inner.step<T>(conversation, tools)
  }

  function submit(
    self,
    conversation: root.ai.Conversation,
    results: root.ai.ToolResult[],
  ) -> root.ai.Conversation throws baml.errors.ToolError | baml.errors.UnknownError {
    self.inner.submit(conversation, results)
  }
}

implements root.ai.ConversationImportProvider for NamedOpenAiToolProvider {
  function import_messages(self, messages: root.ai.Messages)
      -> root.ai.ConversationImport throws baml.errors.UnknownError {
    self.inner.import_messages(messages)
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
    let task = ToolInstructions_task(
      first,
      [knowledge_tool()],
      `Call search_knowledge with query "duplicate charge". After its result the run will move to another provider; that provider must return the final resolution from the imported conversation.`,
    );
    let outcome = task.run(
      runner = root.ai.run.Agent<Resolution>.new(
        tools = [knowledge_tool()],
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
