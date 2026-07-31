
test "guide 06.06: fakes can inject failures before a valid typed output" {
  let fake = ai.testing.FakeProvider { ...fake_resolution(), failures_remaining: 1 };
  let failed = done_or_fail(
    ResolveTicket@task(sample_ticket()).with_provider(fake).run(
      runner = ai.run.Agent<Resolution>.new(),
    ),
  ).value catch (e) {
    _ => "failed",
  };
  assert.equal(failed, "failed")
  assert.equal(
    done_or_fail(
      ResolveTicket@task(sample_ticket()).with_provider(fake).run(
        runner = ai.run.Agent<Resolution>.new(),
      ),
    ).value.category,
    "billing",
  )
}

testset "integ-test-guide-06-failure-injection" {
  test "guide 06.06 live OpenAI: one classified fake failure selects the real fallback exactly once" {
    let injected = ai.testing.FakeProvider {
      ...fake_resolution(),
      provider_name: "injected-primary",
      failures_remaining: 1,
      failure_mode: ai.testing.FakeFailureMode.Retryable,
    };
    let chain = ai.fallback([injected, live_openai()]);
    let response = (
      done_or_fail(
        ResolveTicket@task(sample_ticket()).with_provider(chain).run(
          runner = ai.run.Agent<Resolution>.new(),
        ),
      )
    ).response();
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
