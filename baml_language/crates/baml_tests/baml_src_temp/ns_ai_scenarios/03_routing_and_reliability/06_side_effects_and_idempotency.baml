test "guide 03.06: retry rejects a drive operation without replay evidence" {
  let inner = root.ai.FakeProvider { ...fake_resolution(), failures_remaining: 1 };
  let effectful = EffectfulProvider { inner: inner };
  let label = ResolveTicket_manual(sample_ticket(), root.ai.retry(effectful, 2)) catch (e) {
    let refused: baml.errors.CallError => "cannot-retry",
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
    let terminal: baml.errors.CallError => if (!terminal.is_retryable()) { "terminal" } else { "unexpected" },
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
