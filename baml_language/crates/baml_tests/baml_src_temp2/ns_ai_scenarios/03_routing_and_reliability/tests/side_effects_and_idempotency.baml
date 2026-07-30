// Tests for the side_effects_and_idempotency scenario.
test "guide 03.06: retry rejects a provider-step failure with committed effects" {
  let inner = ai.testing.FakeProvider { ...fake_resolution(), failures_remaining: 1 };
  let effectful = EffectfulProvider { inner: inner };
  let label = ai.internal._require_done(ResolveTicket@task(sample_ticket()).with_provider(ai.retry(effectful, 2)).run(runner = ai.run.Agent<Resolution>.new())).value catch (e) {
    let refused: ai.Failure => "cannot-retry",
    _ => "unexpected",
  };
  assert.equal(label, "cannot-retry")
  assert.equal(inner.calls, 1)
}

test "guide 03.06: a terminal failure is not retried" {
  let failed = ai.testing.FakeProvider {
    ...fake_resolution(),
    failures_remaining: 1,
    failure_mode: ai.testing.FakeFailureMode.Terminal,
  };
  let label = ai.internal._require_done(ResolveTicket@task(sample_ticket()).with_provider(ai.retry(failed, 3)).run(runner = ai.run.Agent<Resolution>.new())).value catch (e) {
    let terminal: ai.Failure => if (!terminal.is_transient()) { "terminal" } else { "unexpected" },
    _ => "unexpected",
  };
  assert.equal(label, "terminal")
  assert.equal(failed.calls, 1)
}

test "guide 03.06: an unknown failure is never assumed replay-safe" {
  let failed = ai.testing.FakeProvider {
    ...fake_resolution(),
    failures_remaining: 1,
    failure_mode: ai.testing.FakeFailureMode.Unknown,
  };
  let label = ai.internal._require_done(ResolveTicket@task(sample_ticket()).with_provider(ai.retry(failed, 3)).run(runner = ai.run.Agent<Resolution>.new())).value catch (e) {
    let unknown: baml.errors.UnknownError => "unknown",
    _ => "unexpected",
  };
  assert.equal(label, "unknown")
  assert.equal(failed.calls, 1)
}

class LiveEffectCommittedFailure {
  request_id: string?,
}

implements ai.Failure for LiveEffectCommittedFailure {
  // Transient in principle — but the committed effect is what blocks replay.
  function is_transient(self) -> bool throws never { true }
  function effects(self) -> ai.Effects throws never { ai.Effects.Committed }
}

class EffectfulAfterLiveResponse {
  inner: root.openai.OpenAIProvider,
  calls: int,
  observed_request_ids: string[],

  implements ai.Provider {
    function name(self) -> string throws never { "effectful-after-openai" }
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
      self.calls = self.calls + 1;
      let response = self.inner.step<T>(conversation, tools);
      self.observed_request_ids.push(response.metadata.request_id ?? "missing");
      throw LiveEffectCommittedFailure { request_id: response.metadata.request_id };
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

testset "integ-test-guide-03-side-effects" {
  test "guide 03.06 live OpenAI: retry refuses replay after a response was observed" {
    let effectful = EffectfulAfterLiveResponse {
      inner: live_openai(),
      calls: 0,
      observed_request_ids: [],
    };
    let label = ai.internal._require_done(ResolveTicket@task(sample_ticket()).with_provider(ai.retry(effectful, 2)).run(runner = ai.run.Agent<Resolution>.new())).value catch (e) {
      let refused: ai.Failure => "cannot-retry",
      _ => "unexpected",
    };
    log.info("live replay refusal " + {
      "label": label,
      "calls": effectful.calls,
      "request_ids": effectful.observed_request_ids,
    }.to_string());
    assert.equal(label, "cannot-retry")
    assert.equal(effectful.calls, 1)
    assert.equal(effectful.observed_request_ids.length(), 1)
    assert.is_true(effectful.observed_request_ids[0] != "missing")
  }
}
