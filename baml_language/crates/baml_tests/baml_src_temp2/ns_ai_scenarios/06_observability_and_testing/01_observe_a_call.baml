// Inspect which provider and model produced a support resolution, including
// request and token-usage metadata.
function observe_a_call(
) -> ai.ResponseWithMetadata<Resolution> throws ai.Failure | baml.errors.UnknownError {
    let response = ai
        .internal
        ._response_from_done(
            ai
                .internal
                ._require_done(
                    ResolveTicket@task(sample_ticket()).with_provider(fake_resolution())
                        .run(runner = ai.run.Agent<Resolution>.new()),
                ),
        );
    log.info(response);
    response
}
