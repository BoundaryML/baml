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

interface Job<T> requires Resource {
  function status(self) -> JobStatus throws never
  function poll(self) -> ResponseWithMetadata<T>? throws baml.errors.UnknownError
  function cancel(self) -> null throws baml.errors.UnknownError
  function token(self) -> JobToken throws never
}

interface BackgroundProvider requires Provider {
  function submit<T>(self, task: Task<T>, options: BackgroundOptions) -> Job<T>
    throws baml.errors.UnknownError
  function resume_job<T>(self, token: JobToken) -> Job<T>
    throws baml.errors.UnknownError
}
