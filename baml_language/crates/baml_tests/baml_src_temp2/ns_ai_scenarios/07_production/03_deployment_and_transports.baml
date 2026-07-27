test "guide 07.03: transport configuration stays on the provider" {
  let provider = root.openai.Chat {
    ...root.openai.chat(),
    model: "gpt-5.6-luna",
    api_key: "gateway-token",
    base_url: "https://gateway.example/v1",
    extra_headers: { "X-Tenant": "acme" },
    extra_body: { "region": "us-west" },
  };
  let task = ResolveTicket_task(sample_ticket(), provider);
  assert.equal(task.provider_name(), "openai")
  assert.equal(provider.base_url, "https://gateway.example/v1")
}

testset "integ-test-guide-07-transport-config" {
  test "guide 07.03 live OpenAI: provider transport options reach a real request" {
    //# Configure deployment-specific transport state on the provider
    let provider = root.openai.Chat {
      ...live_openai(),
      base_url: "https://api.openai.com/v1",
      extra_headers: { "X-Baml-Guide": "07.03" },
      extra_body: { "user": "bepv2-guide-07-03" },
    };
    //# Execute an unchanged task through that configured transport
    let task = ResolveTicket_task(sample_ticket(), provider);
    let response = task.run(
      runner = root.ai.run.CompletionWithMeta<Resolution>.new(),
    );
    log.info(baml.json.stringify({
      "guide": "07.03",
      "provider": response.meta.provider,
      "model": response.meta.model,
      "base_url": provider.base_url,
      "custom_header_count": provider.extra_headers?.length() ?? 0,
      "custom_body_count": provider.extra_body?.length() ?? 0,
    }));
    assert.equal(provider.base_url, "https://api.openai.com/v1")
    assert.equal(provider.extra_headers?.get("X-Baml-Guide"), "07.03")
    assert.equal(provider.extra_body?.length(), 1)
    assert.equal(response.meta.provider, "openai")
    assert.equal(response.meta.model, provider.model)
    assert.is_true(response.value.reply.length() > 0)
  }
}
