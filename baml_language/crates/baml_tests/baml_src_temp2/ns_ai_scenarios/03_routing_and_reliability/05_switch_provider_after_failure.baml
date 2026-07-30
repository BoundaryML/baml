// Recover a support request on a healthy provider after the active provider
// fails.
function switch_provider_after_failure() -> ai.ResponseWithMetadata<Resolution> {
  let failed = ai.testing.FakeProvider {
    ...fake_resolution(),
    provider_name: "failed-primary",
    failures_remaining: 1,
  };
  let fallback = ai.testing.FakeProvider {
    ...fake_resolution(),
    provider_name: "healthy-fallback",
  };
  let response = ai.internal._response_from_done(
    ai.internal._require_done(
      ResolveTicket@task(sample_ticket()).with_provider(ai.fallback([failed, fallback])).run(runner = ai.run.Agent<Resolution>.new()),
    ),
  );
  log.info({
    "scenario": "switch_provider_after_failure",
    "primary_calls": failed.calls,
    "completed_by": response.metadata.provider,
  });
  response
}
