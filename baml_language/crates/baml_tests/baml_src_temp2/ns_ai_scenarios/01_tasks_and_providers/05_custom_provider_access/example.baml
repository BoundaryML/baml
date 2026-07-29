// Add an application-specific execution strategy when the built-in runners do
// not provide the provider access policy the application needs.
function direct_provider_access() -> root.ai.ResponseWithMetadata<Resolution> {
    let provider = fake_resolution();
    let task = ResolveTicket_task(sample_ticket(), provider);
    let response = task.run(runner = GuideAgentCapability<Resolution> { provider: provider });
    log.info(response);
    response
}
