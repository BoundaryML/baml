test "guide 03.06: retry rejects a completion without replay evidence" {
  let inner = root.ai.FakeProvider { ...fake_resolution(), failures_remaining: 1 };
  let effectful = EffectfulProvider { inner: inner };
  let label = ResolveTicket_manual(sample_ticket(), root.ai.retry(effectful, 2)) catch (e) {
    let refused: root.ai.CallError => "cannot-retry",
    _ => "unexpected",
  };
  assert.equal(label, "cannot-retry")
  assert.equal(inner.calls, 1)
}

test "guide 03.06: a terminal failure is not retried" {
  let failed = root.ai.FakeProvider {
    ...fake_resolution(),
    failures_remaining: 1,
    failure_mode: root.ai.FakeFailureMode.Terminal,
  };
  let label = ResolveTicket_manual(sample_ticket(), root.ai.retry(failed, 3)) catch (e) {
    let terminal: root.ai.CallError => if (!terminal.is_retryable()) { "terminal" } else { "unexpected" },
    _ => "unexpected",
  };
  assert.equal(label, "terminal")
  assert.equal(failed.calls, 1)
}

test "guide 03.06: an unknown failure is never assumed replay-safe" {
  let failed = root.ai.FakeProvider {
    ...fake_resolution(),
    failures_remaining: 1,
    failure_mode: root.ai.FakeFailureMode.Unknown,
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
  function is_retryable(self) -> bool throws never { true }
  function is_effectful(self) -> bool throws never { true }
  function is_policy_refusal(self) -> bool throws never { false }
  function is_resumable(self) -> bool throws never { false }
  function is_unsupported(self) -> bool throws never { false }
}

implements root.ai.CallError for LiveEffectCommittedFailure {
  function is_network_error(self) -> bool throws never { true }
  function is_rate_limit(self) -> bool throws never { false }
  function is_parse_error(self) -> bool throws never { false }
}

class EffectfulAfterLiveResponse {
  inner: root.ai.OpenAi,
  calls: int,
  observed_request_ids: string[],

  implements root.ai.Provider {
    function name(self) -> string throws never { "effectful-after-openai" }
    function render_shorthand(self) -> string throws never { self.inner.render_shorthand() }
  }

  implements root.ai.CompletionProvider {
    function complete<T>(self, task: root.ai.Task<T>) -> root.ai.Response<T>
        throws root.ai.CallError | baml.errors.UnknownError {
      self.calls = self.calls + 1;
      let response = self.inner.complete<T>(task.with_provider(self.inner));
      self.observed_request_ids.push(response.meta.request_id ?? "missing");
      throw LiveEffectCommittedFailure { request_id: response.meta.request_id };
    }

    function replay_policy<T>(self, task: root.ai.Task<T>) -> root.ai.ReplayPolicy throws never {
      root.ai.ReplayPolicy { kind: root.ai.ReplayKind.Never, idempotency_key: null }
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
      let refused: root.ai.CallError => "cannot-retry",
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
