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

interface Job<T> requires root.ai.Resource {
  function status(self) -> JobStatus throws never
  function poll(self) -> root.ai.ResponseWithMetadata<T>? throws root.ai.Failure | baml.errors.UnknownError
  function cancel(self) -> null throws root.ai.Failure | baml.errors.UnknownError
  function token(self) -> JobToken throws never
}

interface BackgroundProvider requires root.ai.Provider {
  function submit<T>(self, task: root.ai.Task<T>, options: BackgroundOptions) -> Job<T>
    throws root.ai.Failure | baml.errors.UnknownError
  function resume_job<T>(self, token: JobToken) -> Job<T>
    throws root.ai.Failure | baml.errors.UnknownError
}
