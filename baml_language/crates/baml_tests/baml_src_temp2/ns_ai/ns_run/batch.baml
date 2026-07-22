// A batch consumes the task collection as a whole, so it uses Runner<Task<T>[]>
// directly rather than pretending one Task<T> owns the plural lifecycle.
class Batch<T> {
  provider: root.ai.Provider,
  options: root.ai.BatchOptions,

  function new(
    provider: root.ai.Provider,
    idempotency_key: string? = null,
  ) -> Batch<T> throws never {
    Batch<T> {
      provider: provider,
      options: root.ai.BatchOptions { idempotency_key: idempotency_key },
    }
  }

  implements root.ai.Runner<root.ai.Task<T>[]> {
    type Output = root.ai.Batch<T>
    type Error = baml.errors.UnknownError | baml.errors.Unsupported

    function run(self, tasks: root.ai.Task<T>[]) -> root.ai.Batch<T>
        throws baml.errors.UnknownError | baml.errors.Unsupported {
      root.ai.drivers.submit_batch<T>(self.provider, tasks, self.options)
    }
  }
}
