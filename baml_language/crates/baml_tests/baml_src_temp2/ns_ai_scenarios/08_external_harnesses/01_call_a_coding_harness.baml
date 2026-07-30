// Ask a coding harness to investigate a support issue inside a read-only,
// workspace-scoped sandbox.
function call_a_coding_harness(
) -> ai.harness.HarnessRun<Resolution> throws ai.Failure | baml.errors.UnknownError {
    let harness = fake_model_harness();
    let run = InvestigateRepository@task(sample_ticket()).with_provider(harness.inner)
        .run(
            runner = ai
                .run
                .Harness<Resolution>
                .new(
                    harness,
                    cwd = "/workspace",
                    permission_mode = "read-only",
                    sandbox = "workspace",
                    attributes = { "team": "support" },
                ),
        );
    log.info(run);
    run
}
