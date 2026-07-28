// Recover a support request on a healthy provider after the active provider
// fails.
function switch_provider_after_failure() -> root.ai.ResponseWithMetadata<Resolution> {
  let failed = root.ai.FakeProvider {
    ...fake_resolution(),
    provider_name: "failed-primary",
    failures_remaining: 1,
  };
  let fallback = root.ai.FakeProvider {
    ...fake_resolution(),
    provider_name: "healthy-fallback",
  };
  let response = ResolveTicket_task(
    sample_ticket(),
    root.ai.fallback([failed, fallback]),
  ).run(
    runner = root.ai.run.GenerationWithMeta<Resolution>.new(),
  );
  log.info({
    "scenario": "switch_provider_after_failure",
    "primary_calls": failed.calls,
    "completed_by": response.meta.provider,
  });
  response
}
