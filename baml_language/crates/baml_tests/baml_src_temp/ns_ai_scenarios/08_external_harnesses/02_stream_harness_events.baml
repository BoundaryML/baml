test "guide 08.02: harness results preserve their event stream" {
  let harness = fake_model_harness();
  let run = root.ai.drivers.run_harness<Resolution>(
    InvestigateRepository_task(sample_ticket(), harness), harness_options(),
  );
  assert.equal(run.events[0].kind(), "run_finished")
}

