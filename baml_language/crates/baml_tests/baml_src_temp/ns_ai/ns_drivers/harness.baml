function submit_harness<T>(
  harness: root.ai.Harness,
  task: root.ai.Task<T>,
  options: root.ai.HarnessOptions,
) -> root.ai.HarnessRun<T> throws baml.errors.UnknownError {
  let session = harness.open(options);
  harness.run<T>(session, task)
}
