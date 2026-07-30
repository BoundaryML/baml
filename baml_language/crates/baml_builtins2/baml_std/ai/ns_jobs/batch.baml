class BatchOptions {
    idempotency_key: string?,
}

interface Batch<T> requires ai.Resource {
  function status(self) -> JobStatus throws never
  function results(self) -> ai.ResponseWithMetadata<T>[] throws baml.errors.UnknownError
  function cancel(self) -> null throws baml.errors.UnknownError
}

interface BatchProvider requires ai.Provider {
  function submit_batch<T>(self, tasks: ai.Task<T>[], options: BatchOptions) -> Batch<T>
    throws baml.errors.UnknownError
}
