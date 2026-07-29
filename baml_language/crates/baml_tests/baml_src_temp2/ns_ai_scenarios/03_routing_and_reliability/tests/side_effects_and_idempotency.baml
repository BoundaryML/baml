// Tests for the side_effects_and_idempotency scenario.
test "guide 03.06: retry rejects a provider-step failure with committed effects" {
  let inner = root.ai.testing.FakeProvider { ...fake_resolution(), failures_remaining: 1 };
  let effectful = EffectfulProvider { inner: inner };
  let label = ResolveTicket_manual(sample_ticket(), root.ai.retry(effectful, 2)) catch (e) {
    let refused: root.ai.Failure => "cannot-retry",
    _ => "unexpected",
  };
  assert.equal(label, "cannot-retry")
  assert.equal(inner.calls, 1)
}

test "guide 03.06: a terminal failure is not retried" {
  let failed = root.ai.testing.FakeProvider {
    ...fake_resolution(),
    failures_remaining: 1,
    failure_mode: root.ai.testing.FakeFailureMode.Terminal,
  };
  let label = ResolveTicket_manual(sample_ticket(), root.ai.retry(failed, 3)) catch (e) {
    let terminal: root.ai.Failure => if (!terminal.is_transient()) { "terminal" } else { "unexpected" },
    _ => "unexpected",
  };
  assert.equal(label, "terminal")
  assert.equal(failed.calls, 1)
}

test "guide 03.06: an unknown failure is never assumed replay-safe" {
  let failed = root.ai.testing.FakeProvider {
    ...fake_resolution(),
    failures_remaining: 1,
    failure_mode: root.ai.testing.FakeFailureMode.Unknown,
  };
  let label = ResolveTicket_manual(sample_ticket(), root.ai.retry(failed, 3)) catch (e) {
    let unknown: baml.errors.UnknownError => "unknown",
    _ => "unexpected",
  };
  assert.equal(label, "unknown")
  assert.equal(failed.calls, 1)
}

class LiveEffectCommittedFailure {
  request_id: string?,
}

implements root.ai.Failure for LiveEffectCommittedFailure {
  // Transient in principle — but the committed effect is what blocks replay.
  function is_transient(self) -> bool throws never { true }
  function effects(self) -> root.ai.Effects throws never { root.ai.Effects.Committed }
}

class EffectfulAfterLiveResponse {
  inner: root.openai.Responses,
  calls: int,
  observed_request_ids: string[],

  implements root.ai.Provider {
    function name(self) -> string throws never { "effectful-after-openai" }
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
      self.calls = self.calls + 1;
      let response = self.inner.step<T>(conversation, tools);
      self.observed_request_ids.push(response.metadata.request_id ?? "missing");
      throw LiveEffectCommittedFailure { request_id: response.metadata.request_id };
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

testset "integ-test-guide-03-side-effects" {
  test "guide 03.06 live OpenAI: retry refuses replay after a response was observed" {
    let effectful = EffectfulAfterLiveResponse {
      inner: live_openai(),
      calls: 0,
      observed_request_ids: [],
    };
    let label = ResolveTicket_manual(
      sample_ticket(),
      root.ai.retry(effectful, 2),
    ) catch (e) {
      let refused: root.ai.Failure => "cannot-retry",
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
