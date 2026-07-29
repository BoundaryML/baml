// Tests for the retry_safe_calls scenario.
test "guide 03.01: retry repeats only a replay-safe direct call" {
  //# Configure one transient failure
  let flaky = root.ai.testing.FakeProvider { ...fake_resolution(), failures_remaining: 1 };
  //# Execute through a bounded retry wrapper
  let value = ResolveTicket_manual(sample_ticket(), root.ai.retry(flaky, 2));
  //# Verify the value and attempt count
  assert.equal(value.category, "billing")
  assert.equal(flaky.calls, 2)
}

function _retry_wrong_owner_step(
  provider: root.ai.Retry,
  conversation: root.ai.Conversation,
) -> string throws never {
  provider.step<Resolution>(conversation, []) catch (e) {
    let unknown: baml.errors.UnknownError => return "rejected",
    _ => return "other",
  };
  "accepted"
}

test "guide 03.01: retry conversations require the exact wrapper instance" {
  let inner = fake_resolution();
  let owner = root.ai.retry(inner, 1);
  let other = root.ai.retry(inner, 1);
  let conversation = owner.begin<Resolution>(
    ResolveTicket_task(sample_ticket(), owner),
  );
  assert.equal(_retry_wrong_owner_step(other, conversation), "rejected")
}

class FlakyOnceLiveAgent {
  inner: root.openai.OpenAIProvider,
  attempts: int,

  implements root.ai.Provider {
    function name(self) -> string throws never { "flaky-once-openai" }
    function render_shorthand(self) -> string throws never { self.inner.render_shorthand() }
  }

  implements root.ai.AgentProvider {
    function begin<T>(self, task: root.ai.Task<T>) -> root.ai.Conversation
        throws root.ai.Failure | baml.errors.UnknownError {
      self.inner.begin<T>(task.with_provider(self.inner))
    }

    function step<T>(
      self,
      conversation: root.ai.Conversation,
      tools: root.ai.tools.Tool[],
    ) -> root.ai.ModelStep<T> throws root.ai.Failure | baml.errors.UnknownError {
      self.attempts = self.attempts + 1;
      if (self.attempts == 1) {
        throw root.ai.RateLimited {
          provider: "flaky-once-openai",
          retry_after_ms: 10,
        };
      }
      self.inner.step<T>(conversation, tools)
    }

    function submit(
      self,
      conversation: root.ai.Conversation,
      results: root.ai.tools.ToolResult[],
    ) -> root.ai.Conversation throws root.ai.Failure | baml.errors.UnknownError {
      self.inner.as<root.ai.AgentProvider>.submit(conversation, results)
    }
  }
}

testset "integ-test-guide-03-retry" {
  test "guide 03.01 live OpenAI: a retryable failure causes exactly one retry" {
    let flaky = FlakyOnceLiveAgent { inner: live_openai(), attempts: 0 };
    let task = ResolveTicket_task(sample_ticket(), root.ai.retry(flaky, 2));
    let response = root.ai.internal._response_from_done(
      root.ai.internal._require_done(
        task.run(runner = root.ai.run.Agent<Resolution>.new()),
      ),
    );
    log.info("live retry " + {
      "attempts": flaky.attempts,
      "provider": response.metadata.provider,
      "request_id": response.metadata.request_id,
    }.to_string());
    assert.equal(flaky.attempts, 2)
    assert.equal(response.metadata.provider, "openai")
    assert.is_true(response.value.reply.length() > 0)
  }
}
