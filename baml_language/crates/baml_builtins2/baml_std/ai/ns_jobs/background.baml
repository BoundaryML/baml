class BackgroundOptions {
    idempotency_key: string?,
}

class JobToken {
    provider: string,
    id: string,
    sealed: string?,
}

enum JobStatus {
    Pending,
    Complete,
    Failed,
    Cancelled,
}

interface Job<T> requires ai.Resource {
  function status(self) -> JobStatus throws never
  function poll(self) -> ai.ResponseWithMetadata<T>? throws ai.Failure | baml.errors.UnknownError
  function cancel(self) -> null throws ai.Failure | baml.errors.UnknownError
  function token(self) -> JobToken throws never
}

interface BackgroundProvider requires ai.Provider {
  function submit<T>(self, task: ai.Task<T>, options: BackgroundOptions) -> Job<T>
    throws ai.Failure | baml.errors.UnknownError
  function resume_job<T>(self, token: JobToken) -> Job<T>
    throws ai.Failure | baml.errors.UnknownError
}
