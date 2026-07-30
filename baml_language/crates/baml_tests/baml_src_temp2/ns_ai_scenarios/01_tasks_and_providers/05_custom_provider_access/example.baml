// Add an application-specific execution strategy when the built-in runners do
// not provide the provider access policy the application needs.
function direct_provider_access() -> ai.ResponseWithMetadata<Resolution> {
    let provider = fake_resolution();
    let task = ResolveTicket@task(sample_ticket()).with_provider(provider);
    let response = task.run(runner = GuideAgentCapability<Resolution> { provider: provider });
    log.info(response);
    response
}
