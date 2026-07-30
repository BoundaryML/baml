// Tests for the switch_provider_after_failure scenario.
test "guide 03.05: failure-aware switching is an explicit fallback policy" {
  let failed = ai.testing.FakeProvider { ...fake_resolution(), failures_remaining: 1 };
  let good = fake_resolution();
  let task = ResolveTicket@task(sample_ticket()).with_provider(ai.fallback([failed, good]));
  let result = ai.internal._response_from_done(
    ai.internal._require_done(
      task.run(runner = ai.run.Agent<Resolution>.new()),
    ),
  );
  assert.equal(result.value.category, "billing")
}

testset "integ-test-guide-03-switch-after-failure" {
  test "guide 03.05 live Anthropic: retryable failure switches to the bounded fallback" {
    let failed = ai.testing.FakeProvider {
      ...fake_resolution(),
      provider_name: "failed-primary",
      failures_remaining: 1,
    };
    let task = ResolveTicket@task(sample_ticket()).with_provider(ai.fallback([failed, live_anthropic()]));
    let response = ai.internal._response_from_done(
      ai.internal._require_done(
        task.run(runner = ai.run.Agent<Resolution>.new()),
      ),
    );
    log.info("live failure switch " + {
      "primary_calls": failed.calls,
      "completed_by": response.metadata.provider,
    }.to_string());
    assert.equal(failed.calls, 1)
    assert.equal(response.metadata.provider, "anthropic")
    assert.is_true(response.value.reply.length() > 0)
  }
}
