// Show application-visible progress while a coding harness investigates a
// repository.
function harness_event_callbacks(
) -> root.ai.HarnessRun<Resolution> throws baml.errors.UnknownError {
    let harness = fake_model_harness();
    let observed: string[] = [];
    let run = InvestigateRepository_task(sample_ticket(), harness.inner)
        .run(
            runner = root.ai.run.Harness<Resolution>.new(harness, cwd = "/workspace", permission_mode = "read-only", sandbox = "workspace", attributes = { "team": "support" }, on_event = (
                event: root.ai.AgentEvent,
            ) -> null {
                observed.push(event.kind());
                null
            }),
        );
    log.info({ "observed": observed, "run": run });
    run
}
