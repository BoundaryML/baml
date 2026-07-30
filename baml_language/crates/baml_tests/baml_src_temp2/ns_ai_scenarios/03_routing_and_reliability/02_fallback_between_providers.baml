// Still resolve a support ticket when the preferred provider is temporarily
// unavailable.
function fallback_between_providers() -> ai.ResponseWithMetadata<Resolution> {
  let failed = ai.testing.FakeProvider {
    ...fake_resolution(),
    provider_name: "primary",
    failures_remaining: 1,
  };
  let fallback = ai.testing.FakeProvider {
    ...fake_resolution(),
    provider_name: "fallback",
  };
  let response = ai.internal._response_from_done(
    ai.internal._require_done(
      ResolveTicket@task(sample_ticket()).with_provider(ai.fallback([failed, fallback])).run(runner = ai.run.Agent<Resolution>.new()),
    ),
  );
  log.info({
    "scenario": "fallback_between_providers",
    "completed_by": response.metadata.provider,
    "response": response.value,
  });
  response
}
