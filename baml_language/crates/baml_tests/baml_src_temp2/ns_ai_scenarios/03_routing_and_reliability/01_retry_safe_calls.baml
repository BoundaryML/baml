test "guide 03.01: retry repeats only a replay-safe direct call" {
  //# Configure one transient failure
  let flaky = root.ai.FakeProvider { ...fake_resolution(), failures_remaining: 1 };
  //# Execute through a bounded retry wrapper
  let value = ResolveTicket_manual(sample_ticket(), root.ai.retry(flaky, 2));
  //# Verify the value and attempt count
  assert.equal(value.category, "billing")
  assert.equal(flaky.calls, 2)
}

class FlakyOnceLiveGeneration {
  inner: root.ai.OpenAi,
  attempts: int,

  implements root.ai.Provider {
    function name(self) -> string throws never { "flaky-once-openai" }
    function render_shorthand(self) -> string throws never { self.inner.render_shorthand() }
  }

  implements root.ai.GenerationProvider {
    function generate<T>(self, task: root.ai.Task<T>) -> root.ai.Response<T>
        throws baml.errors.CallError | baml.errors.UnknownError {
      self.attempts = self.attempts + 1;
      if (self.attempts == 1) {
        throw root.ai.FakeCallFailure {
          message: "retryable failure before the live provider call",
          retryable: true,
        };
      }
      self.inner.generate<T>(task.with_provider(self.inner))
    }
  }
}

testset "integ-test-guide-03-retry" {
  test "guide 03.01 live OpenAI: a retryable failure causes exactly one retry" {
    let flaky = FlakyOnceLiveGeneration { inner: live_openai(), attempts: 0 };
    let task = ResolveTicket_task(sample_ticket(), root.ai.retry(flaky, 2));
    let response = task.run(
      runner = root.ai.run.GenerationWithMeta<Resolution>.new(),
    );
    log.info("live retry " + {
      "attempts": flaky.attempts,
      "provider": response.meta.provider,
      "request_id": response.meta.request_id,
    }.to_string());
    assert.equal(flaky.attempts, 2)
    assert.equal(response.meta.provider, "openai")
    assert.is_true(response.value.reply.length() > 0)
  }
}
