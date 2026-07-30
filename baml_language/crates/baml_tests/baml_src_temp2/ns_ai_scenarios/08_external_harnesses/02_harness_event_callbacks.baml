// Show application-visible progress while a coding harness investigates a
// repository.
function harness_event_callbacks(
) -> ai.harness.HarnessRun<Resolution> throws ai.Failure | baml.errors.UnknownError {
    let harness = fake_model_harness();
    let observed: string[] = [];
    let run = InvestigateRepository@task(sample_ticket()).with_provider(harness.inner)
        .run(
            runner = ai.run.Harness<Resolution>.new(harness, cwd = "/workspace", permission_mode = "read-only", sandbox = "workspace", attributes = { "team": "support" }, on_event = (
                event: ai.observe.AgentEvent,
            ) -> null {
                observed.push(event.kind());
                null
            }),
        );
    log.info({ "observed": observed, "run": run });
    run
}
