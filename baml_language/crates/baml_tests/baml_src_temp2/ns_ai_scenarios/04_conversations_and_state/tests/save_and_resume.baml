// Tests for the save_and_resume scenario.
class MisownedBeginProvider {
  inner: ai.testing.FakeProvider,

  implements ai.Provider {
    function name(self) -> string throws never { "misowned-begin" }
    function render_shorthand(self) -> string throws never { self.inner.render_shorthand() }
  }

  implements ai.AgentProvider {
    function begin<T>(self, task: ai.Task<T>) -> ai.Conversation throws ai.Failure {
      self.inner.begin<T>(task.with_provider(self.inner))
    }
    function step<T>(
      self,
      conversation: ai.Conversation,
      tools: ai.tools.Tool[],
    ) -> ai.ModelStep<T> throws ai.Failure | baml.errors.UnknownError {
      self.inner.step<T>(conversation, tools)
    }
    function submit(
      self,
      conversation: ai.Conversation,
      results: ai.tools.ToolResult[],
    ) -> ai.Conversation throws ai.Failure {
      self.inner.submit(conversation, results)
    }
  }
}

class StructurallyEqualMisownedBeginProvider {
  implements ai.Provider {
    function name(self) -> string throws never { "same-shape-misowned-begin" }
    function render_shorthand(self) -> string throws never {
      "baml-fallback/fake"
    }
  }

  implements ai.AgentProvider {
    function begin<T>(self, task: ai.Task<T>) -> ai.Conversation throws never {
      ai.ProviderConversation {
        // A fresh empty provider compares structurally equal to self, but it
        // is not the provider instance selected by the Agent.
        owner: StructurallyEqualMisownedBeginProvider {},
        history: task.messages().to_history(),
        sealed_state: {},
        declared_output_type: ai.output_fingerprint<T>(),
      }
    }
    function step<T>(
      self,
      conversation: ai.Conversation,
      tools: ai.tools.Tool[],
    ) -> ai.ModelStep<T> throws never {
      baml.sys.panic("ownership validation must run before step")
    }
    function submit(
      self,
      conversation: ai.Conversation,
      results: ai.tools.ToolResult[],
    ) -> ai.Conversation throws never {
      conversation
    }
  }
}

class MisownedImportProvider {
  inner: ai.testing.FakeToolProvider,

  implements ai.Provider {
    function name(self) -> string throws never { "misowned-import" }
    function render_shorthand(self) -> string throws never { self.inner.render_shorthand() }
  }

  implements ai.AgentProvider {
    function begin<T>(self, task: ai.Task<T>) -> ai.Conversation throws ai.Failure {
      self.inner.begin<T>(task.with_provider(self.inner))
    }
    function step<T>(
      self,
      conversation: ai.Conversation,
      tools: ai.tools.Tool[],
    ) -> ai.ModelStep<T> throws ai.Failure | baml.errors.UnknownError {
      self.inner.step<T>(conversation, tools)
    }
    function submit(
      self,
      conversation: ai.Conversation,
      results: ai.tools.ToolResult[],
    ) -> ai.Conversation throws baml.errors.UnknownError {
      self.inner.submit(conversation, results)
    }
  }

  implements ai.ConversationImportProvider {
    function import_messages<T>(
      self,
      messages: ai.Messages,
    ) -> ai.ConversationImport throws never {
      self.inner.import_messages<T>(messages)
    }
  }
}

class MisownedSubmitProvider {
  wrong_owner: ai.testing.FakeToolProvider,
  submit_calls: int,

  implements ai.Provider {
    function name(self) -> string throws never { "misowned-submit" }
    function render_shorthand(self) -> string throws never {
      self.wrong_owner.render_shorthand()
    }
  }

  implements ai.AgentProvider {
    function begin<T>(self, task: ai.Task<T>) -> ai.Conversation throws never {
      ai.ProviderConversation {
        owner: self,
        history: task.messages().to_history(),
        sealed_state: {},
        declared_output_type: ai.output_fingerprint<T>(),
      }
    }
    function step<T>(
      self,
      conversation: ai.Conversation,
      tools: ai.tools.Tool[],
    ) -> ai.ModelStep<T> throws never {
      ai.ModelStep<T> {
        outcome: ai.tools.ToolCalls { calls: [one_knowledge_call()] },
        metadata: ai.ResponseMetadata {
          provider: self.name(),
          model: "fake",
          request_id: "misowned-submit-step",
          finish_reason: null,
          usage: null,
          attributes: {},
          raw: null,
        },
      }
    }
    function submit(
      self,
      conversation: ai.Conversation,
      results: ai.tools.ToolResult[],
    ) -> ai.Conversation throws never {
      self.submit_calls += 1;
      ai.ProviderConversation {
        owner: self.wrong_owner,
        history: conversation.messages().to_history(),
        sealed_state: {},
        declared_output_type: conversation.output_type_fingerprint(),
      }
    }
  }
}

test "guide 04.05: Agent rejects begin state owned by another provider instance" {
  let inner = fake_resolution();
  let wrapper = MisownedBeginProvider { inner: inner };
  let rejected = false;
  let _ = ResolveTicket@task(sample_ticket()).with_provider(wrapper)
    .run(runner = ai.run.Agent<Resolution>.new()) catch (e) {
      let failure: ai.Failure => {
        rejected = failure is ai.InvalidRequest;
        null
      },
    };
  assert.is_true(rejected);
  assert.equal(inner.calls, 0)
}

test "guide 04.05: provider ownership uses object identity, not structural equality" {
  let provider = StructurallyEqualMisownedBeginProvider {};
  let rejected = false;
  let _ = ResolveTicket@task(sample_ticket()).with_provider(provider)
    .run(runner = ai.run.Agent<Resolution>.new()) catch (e) {
      let failure: ai.Failure => {
        rejected = failure is ai.InvalidRequest;
        null
      },
    };
  assert.is_true(rejected)
}

test "guide 04.05: Agent rejects imported state owned by another provider instance" {
  let first = fake_tool_provider([one_knowledge_call()]);
  let inner = fake_tool_provider([]);
  let wrapper = MisownedImportProvider { inner: inner };
  let rejected = false;
  let _ = ResolveTicketWithTools@task(sample_ticket()).with_provider(first).with_tools([search_knowledge])
    .run(
      runner = ai.run.Agent<Resolution>.new(
        tools = [search_knowledge],
        prepare_step = (context) -> {
          ai.tools.StepPlan {
            provider: if (context.step == 1) { wrapper } else { null },
            tools: null,
            stop: null,
          }
        },
      ),
    ) catch (e) {
      let failure: ai.Failure => {
        rejected = failure is ai.InvalidRequest;
        null
      },
    };
  assert.is_true(rejected)
}

test "guide 04.05: Agent rejects submitted state owned by another provider instance" {
  let provider = MisownedSubmitProvider {
    wrong_owner: fake_tool_provider([]),
    submit_calls: 0,
  };
  //# The bad submit happens after a committed step: an involuntary stop
  let outcome = ResolveTicketWithTools@task(sample_ticket()).with_provider(provider).with_tools([search_knowledge])
    .run(runner = ai.run.Agent<Resolution>.new(tools = [search_knowledge]));
  let rejected = match (outcome) {
    let failed: ai.Failed => {
      match (failed.cause) {
        let invalid: ai.InvalidRequest => true,
        _ => false,
      }
    },
    _ => false,
  };
  assert.is_true(rejected);
  assert.equal(provider.submit_calls, 1)
}

test "guide 04.05: output fingerprints include nominal and structural identity" {
  let resolution = ai.output_fingerprint<Resolution>();
  let text = ai.output_fingerprint<string>();
  assert.contains(resolution, "baml-output/v1;");
  assert.contains(resolution, "Resolution");
  assert.is_true(resolution != text)
}

test "guide 04.05: OpenAI seals and restores its exact provider conversation" {
  //# Begin a native provider conversation
  let provider = root.openai.OpenAIProvider {
    ...root.openai.responses(),
    model: "gpt-5.6-luna",
    api_key: "unused",
  };
  let conversation = provider.begin<Resolution>(
    ResolveTicketWithTools@task(sample_ticket()).with_provider(provider).with_tools([search_knowledge]),
  );
  //# Save and restore provider-owned wire state
  let token = provider.save_conversation(conversation);
  let restored = provider.restore_conversation(token);
  //# Verify restored provider ownership
  assert.equal(restored.provider().name(), "openai")
}

test "guide 04.05: an agent resumes the supplied conversation instead of starting over" {
  //# Advance a provider conversation through its first turn
  let provider = fake_tool_provider([one_knowledge_call()]);
  let task = ResolveTicketWithTools@task(sample_ticket()).with_provider(provider).with_tools([search_knowledge]);
  let conversation = provider.begin<Resolution>(task);
  let tools = ai.tools.normalize_tools([search_knowledge]);
  let first = provider.step<Resolution>(conversation, tools);
  let calls = match (first.outcome) {
    let value: ai.tools.ToolCalls => value,
    let value: Resolution => baml.sys.panic("expected first tool turn"),
  };
  //# Submit tool results before handing control to the agent
  let results = calls.calls.map((call: ai.tools.ToolCall) -> ai.tools.ToolResult {
    ai.tools.invoke_tool(tools[0], call)
  });
  let resumed = provider.submit(conversation, results);
  let dispatched: string[] = [];
  //# Adopt the hand-driven continuation state into a session and resume
  let session = ai.run.AgentSession<Resolution>.from(task, resumed);
  let outcome = session.resume(
    runner = ai.run.Agent<Resolution>.new(
      tools = [search_knowledge],
      after_tool_call = (event) -> {
        dispatched.push(event.call.name);
        null
      },
    ),
  );
  match (outcome) {
    let done: ai.Done<Resolution> => assert.equal(done.value.category, "billing"),
    _ => assert.is_true(false),
  }
  //# Verify the completed tool call was not replayed
  assert.equal(dispatched.length(), 0)
}

test "guide 04.05: conversation tokens validate provider ownership" {
  //# Construct a token owned by another provider
  let provider = root.openai.OpenAIProvider {
    ...root.openai.responses(),
    model: "gpt-5.6-luna",
    api_key: "unused",
  };
  let wrong = ai.ConversationToken { provider: "anthropic", version: 1, sealed: "{}" };
  //# Reject restoration under the wrong provider
  let label = provider.restore_conversation(wrong) catch (e) {
    let unknown: baml.errors.UnknownError => "rejected",
  };
  assert.equal(label, "rejected")
}

test "guide 04.05: save requires the owner instance and restore requires the same configuration" {
  let owner = root.openai.responses(model = "gpt-5.6-luna", api_key = "owner-key");
  let same_config = root.openai.responses(model = "gpt-5.6-luna", api_key = "rotated-key");
  let other_config = root.openai.responses(
    model = "gpt-5.6-luna",
    api_key = "owner-key",
    base_url = "https://other-account.example.test",
  );
  let conversation = owner.begin<Resolution>(
    ResolveTicket@task(sample_ticket()).with_provider(owner),
  );

  let wrong_owner_rejected = false;
  let _ = same_config.save_conversation(conversation) catch (e) {
    let unknown: baml.errors.UnknownError => {
      wrong_owner_rejected = true;
      ai.ConversationToken { provider: "rejected", version: 0, sealed: "" }
    },
  };
  assert.is_true(wrong_owner_rejected);

  // Credentials may rotate across processes, but request-shaping identity may
  // not silently change for an opaque continuation.
  let token = owner.save_conversation(conversation);
  let restored = same_config.restore_conversation(token);
  assert.equal(restored.provider(), same_config);

  let wrong_config_rejected = false;
  let _ = other_config.restore_conversation(token) catch (e) {
    let unknown: baml.errors.UnknownError => {
      wrong_config_rejected = true;
      conversation
    },
  };
  assert.is_true(wrong_config_rejected)
}

test "guide 04.05: Anthropic and Google apply the same token ownership boundary" {
  let anthropic_owner = root.anthropic.messages(
    model = "claude-test",
    api_key = "owner-key",
    base_url = "https://anthropic.example.test",
  );
  let anthropic_other = root.anthropic.messages(
    model = "claude-test",
    api_key = "other-key",
    base_url = "https://anthropic.example.test",
  );
  let anthropic_conversation = anthropic_owner.begin<Resolution>(
    ResolveTicket@task(sample_ticket()).with_provider(anthropic_owner),
  );
  let anthropic_rejected = false;
  let _ = anthropic_other.save_conversation(anthropic_conversation) catch (e) {
    let unknown: baml.errors.UnknownError => {
      anthropic_rejected = true;
      ai.ConversationToken { provider: "rejected", version: 0, sealed: "" }
    },
  };
  assert.is_true(anthropic_rejected);

  let google_owner = root.google.vertex.gemini(
    model = "gemini-test",
    api_key = "owner-key",
  );
  let google_other = root.google.vertex.gemini(
    model = "gemini-test",
    api_key = "other-key",
  );
  let google_conversation = google_owner.begin<Resolution>(
    ResolveTicket@task(sample_ticket()).with_provider(google_owner),
  );
  let google_rejected = false;
  let _ = google_other.save_conversation(google_conversation) catch (e) {
    let unknown: baml.errors.UnknownError => {
      google_rejected = true;
      ai.ConversationToken { provider: "rejected", version: 0, sealed: "" }
    },
  };
  assert.is_true(google_rejected)
}

test "guide 04.05: an Agent rejects a conversation created for another output type" {
  let provider = fake_resolution();
  let conversation = provider.begin<Resolution>(
    ResolveTicket@task(sample_ticket()).with_provider(provider),
  );
  let wrong_task = ai.task<string>(
    provider,
    (ctx: baml.llm.Context) -> baml.llm.PromptAst {
      baml.llm.assemble_prompt_ast(["Return text.\n", ""], [ctx.output_format])
    },
  );
  let rejected = false;
  let _ = ai.run.AgentSession<string>.from(wrong_task, conversation) catch (e) {
    let invalid: ai.InvalidRequest => {
      rejected = true;
      ai.run.AgentSession<string> { _task: wrong_task, _conversation: conversation, _busy: false }
    },
  };
  assert.is_true(rejected);
  assert.equal(provider.calls, 0)
}

testset "integ-test-guide-04-save-and-resume" {
  test "guide 04.05 live OpenAI: save after a tool request and resume to completion" {
    let provider = live_openai();
    let task = ToolInstructions@task(`Call search_knowledge exactly once with query "duplicate charge". After its result, return the final resolution.`).with_provider(provider).with_tools([search_knowledge]);
    let conversation = provider.begin<Resolution>(task);
    let tools = ai.tools.normalize_tools([search_knowledge]);
    let first = provider.step<Resolution>(conversation, tools);
    let calls = match (first.outcome) {
      let value: ai.tools.ToolCalls => value,
      let value: Resolution => baml.sys.panic("expected a live tool request before save"),
    };
    let token = provider.save_conversation(conversation);
    let restored = provider.restore_conversation(token);
    let results = calls.calls.map((call: ai.tools.ToolCall) -> ai.tools.ToolResult {
      ai.tools.invoke_tool(tools[0], call)
    });
    let resumed = provider
      .as<ai.AgentProvider>
      .submit(restored, results);
    let dispatched: string[] = [];
    let session = ai.run.AgentSession<Resolution>.from(task, resumed);
    let outcome = session.resume(
      runner = ai.run.Agent<Resolution>.new(
        tools = [search_knowledge],
        after_tool_call = (event) -> {
          dispatched.push(event.call.name);
          null
        },
      ),
    );
    log.info("live conversation resume " + {
      "saved_provider": token.provider,
      "saved_bytes": token.sealed.length(),
      "pre_resume_calls": calls.calls.map((call: ai.tools.ToolCall) -> string { call.name }),
      "replayed_dispatches": dispatched,
    }.to_string());
    assert.equal(token.provider, "openai")
    assert.equal(dispatched.length(), 0)
    match (outcome) {
      let done: ai.Done<Resolution> => assert.is_true(done.value.reply.length() > 0),
      _ => assert.is_true(false),
    }
  }
}
