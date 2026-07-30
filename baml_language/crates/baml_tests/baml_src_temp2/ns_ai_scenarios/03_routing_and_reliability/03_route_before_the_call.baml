// Choose the appropriate provider from task metadata before sending any model
// request.
function route_before_the_call() -> ai.ResponseWithMetadata<Resolution> {
  let fast = ai.testing.FakeProvider {
    ...fake_resolution(),
    provider_name: "fast",
  };
  let careful = ai.testing.FakeProvider {
    ...fake_resolution(),
    provider_name: "careful",
  };
  let task = ai.Task<Resolution> {
    ...ResolveTicket@task(sample_ticket()).with_provider(fast),
    tags: { "tier": "pro" },
  };
  let selected = if (task.tags.get("tier") == "pro") { careful } else { fast };
  let routed = task.with_provider(selected);
  let response = ai.internal._response_from_done(
    ai.internal._require_done(
      routed.run(runner = ai.run.Agent<Resolution>.new()),
    ),
  );
  log.info({
    "scenario": "route_before_the_call",
    "selected": routed.provider_name(),
    "response": response.value,
  });
  response
}
