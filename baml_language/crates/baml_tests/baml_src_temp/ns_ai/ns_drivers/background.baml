function submit_background<T>(
  task: root.ai.Task<T>,
  options: root.ai.BackgroundOptions,
) -> root.ai.Job<T> throws baml.errors.UnknownError | baml.errors.Unsupported {
  match (task.provider) {
    let provider: root.ai.BackgroundProvider => provider.submit<T>(task, options),
    _ => throw baml.errors.Unsupported {
      message: "provider cannot submit background work: " + task.provider_name(),
    },
  }
}

