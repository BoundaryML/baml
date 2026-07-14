test "guide 07.03: transport configuration stays on the provider" {
  let provider = root.ai.OpenAi {
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

