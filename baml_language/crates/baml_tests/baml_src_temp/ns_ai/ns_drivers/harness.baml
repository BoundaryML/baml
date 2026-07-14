function run_harness<T>(
  task: root.ai.Task<T>,
  options: root.ai.HarnessOptions,
) -> root.ai.HarnessRun<T> throws baml.errors.UnknownError | baml.errors.Unsupported {
  match (task.provider) {
    let harness: root.ai.HarnessProvider => {
      let session = harness.open(options);
      harness.run<T>(session, task)
    },
    _ => throw baml.errors.Unsupported {
      message: "provider is not a harness: " + task.provider_name(),
    },
  }
}

