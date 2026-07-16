test "guide 08.02: harness results preserve their event stream" {
  let harness = fake_model_harness();
  let run = root.ai.drivers.submit_harness<Resolution>(
    harness,
    InvestigateRepository_task(sample_ticket(), harness.inner),
    harness_options(),
  );
  assert.equal(run.events[0].kind(), "run_finished")
}
