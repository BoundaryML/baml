class BackgroundOptions {
  idempotency_key: string?,
}

class JobToken {
  provider: string,
  id: string,
  sealed: string?,
}

enum JobStatus {
  Pending
  Complete
  Failed
  Cancelled
}

interface Job<T> {
  function status(self) -> JobStatus throws never
  function poll(self) -> Response<T>? throws baml.errors.UnknownError
  function cancel(self) -> null throws baml.errors.UnknownError
  function token(self) -> JobToken throws never
}

interface BackgroundProvider requires Provider {
  function submit<T>(self, task: Task<T>, options: BackgroundOptions) -> Job<T>
    throws baml.errors.UnknownError
  function resume_job<T>(self, token: JobToken) -> Job<T>
    throws baml.errors.UnknownError
}

class FakeJob<T> {
  job_token: JobToken,
  response: Response<T>,
  cancelled: bool,

  implements Job<T> {
    function status(self) -> JobStatus throws never {
      if (self.cancelled) { JobStatus.Cancelled } else { JobStatus.Complete }
    }
    function poll(self) -> Response<T>? throws never {
      if (self.cancelled) { null } else { self.response }
    }
    function cancel(self) -> null throws never { self.cancelled = true; null }
    function token(self) -> JobToken throws never { self.job_token }
  }
}

class FakeBackgroundProvider {
  inner: FakeProvider,
}

implements Provider for FakeBackgroundProvider {
  function name(self) -> string throws never { "fake-background" }
  function render_shorthand(self) -> string throws never { self.inner.render_shorthand() }
}

implements BackgroundProvider for FakeBackgroundProvider {
  function submit<T>(self, task: Task<T>, options: BackgroundOptions) -> Job<T>
      throws baml.errors.UnknownError {
    FakeJob<T> {
      job_token: JobToken {
        provider: self.name(),
        id: options.idempotency_key ?? "fake-job",
        sealed: self.inner.output,
      },
      response: self.inner.generate<T>(task.with_provider(self.inner)),
      cancelled: false,
    }
  }

  function resume_job<T>(self, token: JobToken) -> Job<T> throws baml.errors.UnknownError {
    let task = root.ai.task<T>(self.inner, (ctx: baml.llm.Context) -> baml.llm.PromptAst {
      baml.llm.assemble_prompt_ast(["resume"], [])
    });
    self.submit<T>(task, BackgroundOptions { idempotency_key: token.id })
  }
}

