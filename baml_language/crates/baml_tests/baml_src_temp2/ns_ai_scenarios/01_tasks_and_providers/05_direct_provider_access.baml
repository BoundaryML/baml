test "guide 01.05: a custom driver can call a known capability directly" {
  let provider = fake_resolution();
  let task = ResolveTicket_task(sample_ticket(), provider);
  let response = task.drive(
    guide_generation_capability<Resolution>(provider),
  );
  assert.equal(response.meta.provider, "fake")
  assert.equal(response.value.category, "billing")
}

// A package-defined driver participates in exactly the same Task.drive
// protocol as a built-in driver. It can still call a provider capability
// directly when it needs custom lifecycle policy.
class GuideGenerationCapability<T> {
  provider: root.ai.GenerationProvider,
}

implements<T> root.ai.Driver<root.ai.Task<T>> for GuideGenerationCapability<T> {
  type Output = root.ai.Response<T>
  type Error = baml.errors.CallError | baml.errors.UnknownError

  function drive(self, task: root.ai.Task<T>) -> root.ai.Response<T>
      throws baml.errors.CallError | baml.errors.UnknownError {
    self.provider.generate<T>(task.with_provider(self.provider))
  }
}

function guide_generation_capability<T>(
  provider: root.ai.GenerationProvider,
) -> GuideGenerationCapability<T> throws never {
  GuideGenerationCapability<T> { provider: provider }
}

testset "integ-test-guide-01-direct-provider-access" {
  test "guide 01.05 live OpenAI: a custom driver calls GenerationProvider directly" {
    let provider = live_openai();
    let response = ResolveTicket_task(sample_ticket(), provider).drive(
      guide_generation_capability<Resolution>(provider),
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
