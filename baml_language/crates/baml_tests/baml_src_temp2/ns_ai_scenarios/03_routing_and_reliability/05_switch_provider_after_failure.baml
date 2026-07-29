// Recover a support request on a healthy provider after the active provider
// fails.
function switch_provider_after_failure() -> root.ai.ResponseWithMetadata<Resolution> {
  let failed = root.ai.testing.FakeProvider {
    ...fake_resolution(),
    provider_name: "failed-primary",
    failures_remaining: 1,
  };
  let fallback = root.ai.testing.FakeProvider {
    ...fake_resolution(),
    provider_name: "healthy-fallback",
  };
  let response = root.ai.internal._response_from_done(
    root.ai.internal._require_done(
      ResolveTicket_task(
        sample_ticket(),
        root.ai.fallback([failed, fallback]),
      ).run(runner = root.ai.run.Agent<Resolution>.new()),
    ),
  );
  log.info({
    "scenario": "switch_provider_after_failure",
    "primary_calls": failed.calls,
    "completed_by": response.metadata.provider,
  });
  response
}
