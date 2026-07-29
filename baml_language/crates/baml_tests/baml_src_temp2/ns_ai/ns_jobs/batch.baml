class BatchOptions {
    idempotency_key: string?,
}

interface Batch<T> requires root.ai.Resource {
  function status(self) -> JobStatus throws never
  function results(self) -> root.ai.ResponseWithMetadata<T>[] throws baml.errors.UnknownError
  function cancel(self) -> null throws baml.errors.UnknownError
}

interface BatchProvider requires root.ai.Provider {
  function submit_batch<T>(self, tasks: root.ai.Task<T>[], options: BatchOptions) -> Batch<T>
    throws baml.errors.UnknownError
}
