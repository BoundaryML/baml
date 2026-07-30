// Submit long-running support work for remote execution and keep a durable token
// that can be polled or resumed later.
function background_jobs(
) -> ai.jobs.JobToken throws ai.Failure | baml.errors.UnknownError {
    let provider = ai.testing.FakeBackgroundProvider { inner: fake_resolution() };
    let job = ResolveTicket@task(sample_ticket()).with_provider(provider)
        .run(runner = ai.run.Background<Resolution>.new(idempotency_key = "ticket-T-100"));
    log.info({ "status": job.status(), "response": job.poll(), "token": job.token() });
    job.token()
}
