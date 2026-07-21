// A batch consumes the task collection as a whole, so it uses Driver<Task<T>[]>
// directly rather than pretending one Task<T> owns the plural lifecycle.
class BatchSubmission<T> {
  provider: root.ai.Provider,
  options: root.ai.BatchOptions,
}

implements<T> root.ai.Driver<root.ai.Task<T>[]> for BatchSubmission<T> {
  type Output = root.ai.Batch<T>
  type Error = baml.errors.UnknownError | baml.errors.Unsupported

  function drive(self, tasks: root.ai.Task<T>[]) -> root.ai.Batch<T>
      throws baml.errors.UnknownError | baml.errors.Unsupported {
    root.ai.drivers.submit_batch<T>(self.provider, tasks, self.options)
  }
}

function batch<T>(
  provider: root.ai.Provider,
  options: root.ai.BatchOptions,
) -> BatchSubmission<T> throws never {
  BatchSubmission<T> { provider: provider, options: options }
}
