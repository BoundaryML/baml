// Ask a coding harness to investigate a support issue inside a read-only,
// workspace-scoped sandbox.
function call_a_coding_harness(
) -> root.ai.harness.HarnessRun<Resolution> throws root.ai.Failure | baml.errors.UnknownError {
    let harness = fake_model_harness();
    let run = InvestigateRepository_task(sample_ticket(), harness.inner)
        .run(
            runner = root
                .ai
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
