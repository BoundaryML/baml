function submit_batch<T>(
  tasks: root.ai.Task<T>[],
  provider: root.ai.Provider,
  options: root.ai.BatchOptions,
) -> root.ai.Batch<T> throws baml.errors.UnknownError | baml.errors.Unsupported {
  match (provider) {
    let batch: root.ai.BatchProvider => batch.submit_batch<T>(tasks, options),
    _ => throw baml.errors.Unsupported { message: "provider cannot submit batches" },
  }
}

