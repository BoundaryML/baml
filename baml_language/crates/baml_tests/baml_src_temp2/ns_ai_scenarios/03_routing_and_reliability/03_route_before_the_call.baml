// Choose the appropriate provider from task metadata before sending any model
// request.
function route_before_the_call() -> root.ai.ResponseWithMetadata<Resolution> {
  let fast = root.ai.testing.FakeProvider {
    ...fake_resolution(),
    provider_name: "fast",
  };
  let careful = root.ai.testing.FakeProvider {
    ...fake_resolution(),
    provider_name: "careful",
  };
  let task = root.ai.Task<Resolution> {
    ...ResolveTicket_task(sample_ticket(), fast),
    tags: { "tier": "pro" },
  };
  let selected = if (task.tags.get("tier") == "pro") { careful } else { fast };
  let routed = task.with_provider(selected);
  let response = routed.run(
    runner = root.ai.run.GenerationWithMeta<Resolution>.new(),
  );
  log.info({
    "scenario": "route_before_the_call",
    "selected": routed.provider_name(),
    "response": response.value,
  });
  response
}
