class BackgroundOptions { idempotency_key: string? }
class JobToken { provider: string, id: string }
class Pending { retry_after: baml.time.Duration? }
class Failed { error: Failure }
class Cancelled {}

interface Job<T> {
  function poll(self) -> Done<T> | Pending | Failed | Cancelled throws baml.errors.UnknownError
  function cancel(self) -> null throws baml.errors.UnknownError
  function token(self) -> JobToken throws never
  function cleanup(self) -> null throws never
}

interface Background requires Provider {
  function submit<T>(self, request: Request<T>, options: BackgroundOptions) -> Job<T>
    throws baml.errors.UnknownError
  function resume_job<T>(self, token: JobToken) -> Job<T>
    throws baml.errors.UnknownError
}

function submit_background<T>(request: Request<T>, options: BackgroundOptions) -> Job<T>
    throws baml.errors.UnknownError {
  let background = match (request.provider) {
    let capability: Background => capability,
    _ => throw baml.errors.UnknownError {
      data: request.provider,
      message: ["provider cannot submit background work"],
    },
  };
  background.submit<T>(request, options)
}
