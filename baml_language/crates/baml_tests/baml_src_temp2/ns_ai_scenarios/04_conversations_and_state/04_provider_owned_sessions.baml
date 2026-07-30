// Continue later work inside a provider-owned session without manually
// reconstructing its conversation state.
function provider_owned_sessions(
) -> ai.ResponseWithMetadata<Resolution>[] throws ai.Failure | baml.errors.UnknownError {
    let provider = ai.testing.FakeSessionProvider { inner: fake_resolution(), sessions: [] };
    let opened = provider
        .open_session(
            ai.sessions.SessionOptions {
                session_id: "support-T-100",
                attributes: { "team": "support" },
            },
        );
    let first = ResolveTicket@task(sample_ticket()).with_provider(provider)
        .run(runner = ai.run.InSession<Resolution>.new(opened));

    let resumed = provider.resume_session(opened.token());
    let second = ResolveTicket@task(sample_ticket()).with_provider(provider)
        .run(runner = ai.run.InSession<Resolution>.new(resumed));

    let responses = [first, second];
    log.info(
        {
            "opened_session": opened.id(),
            "resumed_session": resumed.id(),
            "responses": responses.map((response) -> {
                response.value
            }),
        },
    );
    responses
}
