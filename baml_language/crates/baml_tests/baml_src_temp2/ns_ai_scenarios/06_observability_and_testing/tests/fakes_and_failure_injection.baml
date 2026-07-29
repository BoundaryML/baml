
test "guide 06.06: fakes can inject failures before a valid typed output" {
  let fake = root.ai.testing.FakeProvider { ...fake_resolution(), failures_remaining: 1 };
  let failed = root.ai.internal._require_done(
    ResolveTicket_task(sample_ticket(), fake).run(
      runner = root.ai.run.Agent<Resolution>.new(),
    ),
  ).value catch (e) {
    _ => "failed",
  };
  assert.equal(failed, "failed")
  assert.equal(
    root.ai.internal._require_done(
      ResolveTicket_task(sample_ticket(), fake).run(
        runner = root.ai.run.Agent<Resolution>.new(),
      ),
    ).value.category,
    "billing",
  )
}

testset "integ-test-guide-06-failure-injection" {
  test "guide 06.06 live OpenAI: one classified fake failure selects the real fallback exactly once" {
    let injected = root.ai.testing.FakeProvider {
      ...fake_resolution(),
      provider_name: "injected-primary",
      failures_remaining: 1,
      failure_mode: root.ai.testing.FakeFailureMode.Retryable,
    };
    let chain = root.ai.fallback([injected, live_openai()]);
    let response = root.ai.internal._response_from_done(
      root.ai.internal._require_done(
        ResolveTicket_task(sample_ticket(), chain).run(
          runner = root.ai.run.Agent<Resolution>.new(),
        ),
      ),
    );
    log.info(baml.json.stringify({
      "guide": "06.06",
      "injected_attempts": injected.calls,
      "selected_provider": response.metadata.provider,
    }));
    assert.equal(injected.calls, 1)
    assert.equal(injected.failures_remaining, 0)
    assert.equal(response.metadata.provider, "openai")
    assert.is_true(response.value.reply.length() > 0)
  }
}
