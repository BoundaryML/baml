test "guide 01.05: a custom driver can call a known capability directly" {
  let provider = fake_resolution();
  let task = ResolveTicket_task(sample_ticket(), provider);
  let response = provider.generate<Resolution>(task);
  assert.equal(response.meta.provider, "fake")
  assert.equal(response.value.category, "billing")
}

function call_generation_capability<T>(
  provider: root.ai.GenerationProvider,
  task: root.ai.Task<T>,
) -> root.ai.Response<T> throws baml.errors.CallError | baml.errors.UnknownError {
  provider.generate<T>(task.with_provider(provider))
}

testset "integ-test-guide-01-direct-provider-access" {
  test "guide 01.05 live OpenAI: a custom driver calls GenerationProvider directly" {
    let provider = live_openai();
    let response = call_generation_capability<Resolution>(
      provider,
      ResolveTicket_task(sample_ticket(), provider),
    );
    log.info("live direct capability " + {
      "provider": response.meta.provider,
      "finish_reason": response.meta.finish_reason,
      "request_id": response.meta.request_id,
    }.to_string());
    assert.equal(response.meta.provider, "openai")
    assert.is_true(response.value.reply.length() > 0)
  }
}
