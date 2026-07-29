
test "guide 07.03: transport configuration stays on the provider" {
  let provider = root.openai.Responses {
    ...root.openai.responses(),
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
    let provider = root.openai.Responses {
      ...live_openai(),
      base_url: "https://api.openai.com/v1",
      extra_headers: { "X-Baml-Guide": "07.03" },
      extra_body: { "user": "bepv2-guide-07-03" },
    };
    //# Execute an unchanged task through that configured transport
    let task = ResolveTicket_task(sample_ticket(), provider);
    let response = root.ai.internal._response_from_done(
      root.ai.internal._require_done(
        task.run(
          runner = root.ai.run.Agent<Resolution>.new(),
        ),
      ),
    );
    log.info(baml.json.stringify({
      "guide": "07.03",
      "provider": response.metadata.provider,
      "model": response.metadata.model,
      "base_url": provider.base_url,
      "custom_header_count": provider.extra_headers?.length() ?? 0,
      "custom_body_count": provider.extra_body?.length() ?? 0,
    }));
    assert.equal(provider.base_url, "https://api.openai.com/v1")
    assert.equal(provider.extra_headers?.get("X-Baml-Guide"), "07.03")
    assert.equal(provider.extra_body?.length(), 1)
    assert.equal(response.metadata.provider, "openai")
    assert.equal(response.metadata.model, provider.model)
    assert.is_true(response.value.reply.length() > 0)
  }
}
