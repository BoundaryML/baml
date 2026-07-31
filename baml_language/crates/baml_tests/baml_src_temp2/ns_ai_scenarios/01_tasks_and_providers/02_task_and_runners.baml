// Resolve a support ticket while retaining the provider metadata that explains
// where the answer came from.
function task_and_runners() -> ai.ResponseWithMetadata<Resolution> {
    let task = ResolveTicket@task(sample_ticket()).with_provider(fake_resolution());
    log.info(
        {
            "identity": task.identity?.name,
            "provider": task.provider_name(),
            "tools": task.tools.length(),
        },
    );
    let response = (
            done_or_fail(task.run(runner = ai.run.Agent<Resolution>.new()))
        ).response();
    log.info(response);
    response
}
