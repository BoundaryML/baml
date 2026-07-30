// Give an existing harness an application-specific name without reimplementing
// its task execution or session controls.
function name_a_harness(
) -> ai.harness.HarnessRun<Resolution> throws ai.Failure | baml.errors.UnknownError {
    let harness = NamedHarness { inner: fake_model_harness() };
    let run = InvestigateRepository@task(sample_ticket()).with_provider(harness.inner.inner)
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
    log.info({ "adapter": harness.label(), "run": run });
    run
}
