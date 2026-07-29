// Still resolve a support ticket when the preferred provider is temporarily
// unavailable.
function fallback_between_providers() -> root.ai.ResponseWithMetadata<Resolution> {
  let failed = root.ai.testing.FakeProvider {
    ...fake_resolution(),
    provider_name: "primary",
    failures_remaining: 1,
  };
  let fallback = root.ai.testing.FakeProvider {
    ...fake_resolution(),
    provider_name: "fallback",
  };
  let response = ResolveTicket_task(
    sample_ticket(),
    root.ai.fallback([failed, fallback]),
  ).run(
    runner = root.ai.run.GenerationWithMeta<Resolution>.new(),
  );
  log.info({
    "scenario": "fallback_between_providers",
    "completed_by": response.metadata.provider,
    "response": response.value,
  });
  response
}
