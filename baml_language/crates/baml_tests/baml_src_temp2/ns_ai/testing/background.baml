class FakeJob<T> {
    job_token: JobToken,
    response: ResponseWithMetadata<T>,
    cancelled: bool,

    // function cleanup(self) -> void throws never { self.cancel(); }

    implements Job<T> {
        function status(self) -> JobStatus throws never {
            if (self.cancelled) {
                JobStatus.Cancelled
            } else {
                JobStatus.Complete
            }
        }

        function poll(self) -> ResponseWithMetadata<T>? throws never {
            if (self.cancelled) {
                null
            } else {
                self.response
            }
        }

        function cancel(self) -> null throws never {
            self.cancelled = true;
            null
        }

        function token(self) -> JobToken throws never {
            self.job_token
        }
    }

    implements Resource {}
}

class FakeBackgroundProvider {
    inner: FakeProvider,
}

implements Provider for FakeBackgroundProvider {
  function name(self) -> string throws never { "fake-background" }
  function render_shorthand(self) -> string throws never { self.inner.render_shorthand() }
}

implements BackgroundProvider for FakeBackgroundProvider {
  function submit<T>(
    self,
    task: Task<T>,
    options: BackgroundOptions,
  ) -> Job<T> throws baml.errors.UnknownError {
    //# Execute immediately but expose the result through a durable job resource
    FakeJob<T> {
      job_token: JobToken {
        provider: self.name(),
        id: options.idempotency_key ?? "fake-job",
        sealed: self.inner.output,
      },
      response: self.inner.generate<T>(task.with_provider(self.inner)) catch (e) {
        _ => throw baml.errors.UnknownError {
          data: e,
          message: ["fake background generation failed"],
        },
      },
      cancelled: false,
    }
  }

  function resume_job<T>(
    self,
    token: JobToken,
  ) -> Job<T> throws baml.errors.UnknownError {
    //# Reconstruct a task from the saved job token
    let task = root.ai.task<T>(
      self.inner,
      (ctx: baml.llm.Context) -> baml.llm.PromptAst {
        baml.llm.assemble_prompt_ast(["resume"], [])
      },
    );
    //# Resubmit with the original durable job identity
    self.submit<T>(
      task,
      BackgroundOptions { idempotency_key: token.id },
    )
  }
}
