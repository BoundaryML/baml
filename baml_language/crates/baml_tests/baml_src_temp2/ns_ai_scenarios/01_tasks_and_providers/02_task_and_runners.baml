// Resolve a support ticket while retaining the provider metadata that explains
// where the answer came from.
function task_and_runners() -> root.ai.ResponseWithMetadata<Resolution> {
    let task = ResolveTicket_task(sample_ticket(), fake_resolution());
    log.info(
        {
            "identity": task.identity?.name,
            "provider": task.provider_name(),
            "tools": task.tools.length(),
        },
    );
    let response = root
        .ai
        .internal
        ._response_from_done(
            root.ai.internal._require_done(task.run(runner = root.ai.run.Agent<Resolution>.new())),
        );
    log.info(response);
    response
}
