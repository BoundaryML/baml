test "guide 03.05: failure-aware switching is an explicit fallback policy" {
  let failed = root.ai.FakeProvider { ...fake_resolution(), failures_remaining: 1 };
  let good = fake_resolution();
  let task = ResolveTicket_task(sample_ticket(), root.ai.fallback([failed, good]));
  let result = task.drive(
    root.ai.driver.generate_with_meta<Resolution>(),
  );
  assert.equal(result.value.category, "billing")
}

testset "integ-test-guide-03-switch-after-failure" {
  test "guide 03.05 live Anthropic: retryable failure switches to the bounded fallback" {
    let failed = root.ai.FakeProvider {
      ...fake_resolution(),
      provider_name: "failed-primary",
      failures_remaining: 1,
    };
    let task = ResolveTicket_task(
      sample_ticket(),
      root.ai.fallback([failed, live_anthropic()]),
    );
    let response = task.drive(
      root.ai.driver.generate_with_meta<Resolution>(),
    );
    log.info("live failure switch " + {
      "primary_calls": failed.calls,
      "completed_by": response.meta.provider,
    }.to_string());
    assert.equal(failed.calls, 1)
    assert.equal(response.meta.provider, "anthropic")
    assert.is_true(response.value.reply.length() > 0)
  }
}
