// Verify recovery from a controlled provider failure without making a real
// model request.
function fakes_and_failure_injection(
) -> Resolution throws ai.Failure | baml.errors.UnknownError {
    let fake = ai.testing.FakeProvider {
        ...fake_resolution(),
        failures_remaining: 1,
    };
    let first_attempt = done_or_fail(
            ResolveTicket@task(sample_ticket()).with_provider(fake)
                .run(runner = ai.run.Agent<Resolution>.new()),
        )
        .value catch (e) {
        _ => null
    };
    log.info({ "first_attempt": first_attempt, "remaining_failures": fake.failures_remaining });
    done_or_fail(
            ResolveTicket@task(sample_ticket()).with_provider(fake)
                .run(runner = ai.run.Agent<Resolution>.new()),
        )
        .value
}
