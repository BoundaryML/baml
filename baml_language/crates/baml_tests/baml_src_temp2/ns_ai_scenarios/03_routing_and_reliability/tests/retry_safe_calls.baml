// Tests for the retry_safe_calls scenario.
test "guide 03.01: retry repeats only a replay-safe direct call" {
  //# Configure one transient failure
  let flaky = ai.testing.FakeProvider { ...fake_resolution(), failures_remaining: 1 };
  //# Execute through a bounded retry wrapper
  let value = ai.internal._require_done(ResolveTicket@task(sample_ticket()).with_provider(ai.retry(flaky, 2)).run(runner = ai.run.Agent<Resolution>.new())).value;
  //# Verify the value and attempt count
  assert.equal(value.category, "billing")
  assert.equal(flaky.calls, 2)
}

function _retry_wrong_owner_step(
  provider: ai.Retry,
  conversation: ai.Conversation,
) -> string throws never {
  provider.step<Resolution>(conversation, []) catch (e) {
    let unknown: baml.errors.UnknownError => return "rejected",
    _ => return "other",
  };
  "accepted"
}

test "guide 03.01: retry conversations require the exact wrapper instance" {
  let inner = fake_resolution();
  let owner = ai.retry(inner, 1);
  let other = ai.retry(inner, 1);
  let conversation = owner.begin<Resolution>(
    ResolveTicket@task(sample_ticket()).with_provider(owner),
  );
  assert.equal(_retry_wrong_owner_step(other, conversation), "rejected")
}

class FlakyOnceLiveAgent {
  inner: root.openai.OpenAIProvider,
  attempts: int,

  implements ai.Provider {
    function name(self) -> string throws never { "flaky-once-openai" }
    function render_shorthand(self) -> string throws never { self.inner.render_shorthand() }
  }

  implements ai.AgentProvider {
    function begin<T>(self, task: ai.Task<T>) -> ai.Conversation
        throws ai.Failure | baml.errors.UnknownError {
      self.inner.begin<T>(task.with_provider(self.inner))
    }

    function step<T>(
      self,
      conversation: ai.Conversation,
      tools: ai.tools.Tool[],
    ) -> ai.ModelStep<T> throws ai.Failure | baml.errors.UnknownError {
      self.attempts = self.attempts + 1;
      if (self.attempts == 1) {
        throw ai.RateLimited {
          provider: "flaky-once-openai",
          retry_after_ms: 10,
        };
      }
      self.inner.step<T>(conversation, tools)
    }

    function submit(
      self,
      conversation: ai.Conversation,
      results: ai.tools.ToolResult[],
    ) -> ai.Conversation throws ai.Failure | baml.errors.UnknownError {
      self.inner.as<ai.AgentProvider>.submit(conversation, results)
    }
  }
}

testset "integ-test-guide-03-retry" {
  test "guide 03.01 live OpenAI: a retryable failure causes exactly one retry" {
    let flaky = FlakyOnceLiveAgent { inner: live_openai(), attempts: 0 };
    let task = ResolveTicket@task(sample_ticket()).with_provider(ai.retry(flaky, 2));
    let response = ai.internal._response_from_done(
      ai.internal._require_done(
        task.run(runner = ai.run.Agent<Resolution>.new()),
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
