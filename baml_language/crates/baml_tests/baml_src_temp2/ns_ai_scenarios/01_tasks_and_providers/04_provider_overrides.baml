// Send one support ticket to a different provider without changing the reusable
// task definition or its typed result.
function provider_overrides() -> root.ai.ResponseWithMetadata<Resolution> {
    let fast = root.ai.testing.FakeProvider {
        ...root.ai.testing.fake_output_provider(`{
            "category":"billing","priority":"Normal","summary":"fast","reply":"fast reply"
        }`),
        provider_name: "fast",
    };
    let careful = root.ai.testing.FakeProvider {
        ...fake_resolution(),
        provider_name: "careful",
    };
    let original = ResolveTicket_task(sample_ticket(), fast);
    let overridden = original.with_provider(careful);
    let response = overridden.run(
        runner = root.ai.run.GenerationWithMeta<Resolution>.new(),
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
