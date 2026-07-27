class BatchOptions {
    idempotency_key: string?,
}

interface Batch<T> requires Resource {
  function status(self) -> JobStatus throws never
  function results(self) -> Response<T>[] throws baml.errors.UnknownError
  function cancel(self) -> null throws baml.errors.UnknownError
}

interface BatchProvider requires Provider {
  function submit_batch<T>(self, tasks: Task<T>[], options: BatchOptions) -> Batch<T>
    throws baml.errors.UnknownError
}
