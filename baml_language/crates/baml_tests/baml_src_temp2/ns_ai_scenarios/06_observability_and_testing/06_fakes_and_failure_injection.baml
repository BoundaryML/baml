// Verify recovery from a controlled provider failure without making a real
// model request.
function fakes_and_failure_injection(
) -> Resolution throws root.ai.Failure | baml.errors.UnknownError {
    let fake = root.ai.testing.FakeProvider {
        ...fake_resolution(),
        failures_remaining: 1,
    };
    let first_attempt = ResolveTicket_task(sample_ticket(), fake)
        .run(runner = root.ai.run.Generation<Resolution>.new()) catch (e) {
        _ => null
    };
    log.info({ "first_attempt": first_attempt, "remaining_failures": fake.failures_remaining });
    ResolveTicket_task(sample_ticket(), fake)
        .run(runner = root.ai.run.Generation<Resolution>.new())
}
