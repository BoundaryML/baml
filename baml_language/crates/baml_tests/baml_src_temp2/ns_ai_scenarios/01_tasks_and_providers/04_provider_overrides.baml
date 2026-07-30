// Send one support ticket to a different provider without changing the reusable
// task definition or its typed result.
function provider_overrides() -> ai.ResponseWithMetadata<Resolution> {
    let fast = ai.testing.FakeProvider {
        ...ai.testing.fake_output_provider(`{
            "category":"billing","priority":"Normal","summary":"fast","reply":"fast reply"
        }`),
        provider_name: "fast",
    };
    let careful = ai.testing.FakeProvider {
        ...fake_resolution(),
        provider_name: "careful",
    };
    let original = ResolveTicket@task(sample_ticket()).with_provider(fast);
    let overridden = original.with_provider(careful);
    let response = ai.internal._response_from_done(
      ai.internal._require_done(
        overridden.run(runner = ai.run.Agent<Resolution>.new()),
      ),
    );
    log.info(
        {
            "before": original.provider_name(),
            "after": overridden.provider_name(),
            "response": response,
        },
    );
    response
}
