// Inspect which provider and model produced a support resolution, including
// request and token-usage metadata.
function observe_a_call(
) -> root.ai.ResponseWithMetadata<Resolution> throws root.ai.Failure | baml.errors.UnknownError {
    let response = ResolveTicket_task(sample_ticket(), fake_resolution())
        .run(runner = root.ai.run.GenerationWithMeta<Resolution>.new());
    log.info(response);
    response
}
