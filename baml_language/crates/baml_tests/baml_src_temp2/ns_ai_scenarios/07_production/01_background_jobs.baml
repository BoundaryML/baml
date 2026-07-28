// Submit long-running support work for remote execution and keep a durable token
// that can be polled or resumed later.
function background_jobs(
) -> root.ai.JobToken throws baml.errors.UnknownError | baml.errors.Unsupported {
    let provider = root.ai.FakeBackgroundProvider { inner: fake_resolution() };
    let job = ResolveTicket_task(sample_ticket(), provider)
        .run(runner = root.ai.run.Background<Resolution>.new(idempotency_key = "ticket-T-100"));
    log.info({ "status": job.status(), "response": job.poll(), "token": job.token() });
    job.token()
}
