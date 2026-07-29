class FakeJob<T> {
    job_token: root.ai.jobs.JobToken,
    response: root.ai.ResponseWithMetadata<T>,
    cancelled: bool,

    // function cleanup(self) -> void throws never { self.cancel(); }

    implements root.ai.jobs.Job<T> {
        function status(self) -> root.ai.jobs.JobStatus throws never {
            if (self.cancelled) {
                root.ai.jobs.JobStatus.Cancelled
            } else {
                root.ai.jobs.JobStatus.Complete
            }
        }

        function poll(self) -> root.ai.ResponseWithMetadata<T>? throws never {
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

        function token(self) -> root.ai.jobs.JobToken throws never {
            self.job_token
        }
    }

    implements root.ai.Resource {}
}

class FakeBackgroundProvider {
    inner: FakeProvider,
}

implements root.ai.Provider for FakeBackgroundProvider {
  function name(self) -> string throws never { "fake-background" }
  function render_shorthand(self) -> string throws never { self.inner.render_shorthand() }
}

implements root.ai.jobs.BackgroundProvider for FakeBackgroundProvider {
  function submit<T>(
    self,
    task: root.ai.Task<T>,
    options: root.ai.jobs.BackgroundOptions,
  ) -> root.ai.jobs.Job<T> throws baml.errors.UnknownError {
    //# Execute immediately but expose the result through a durable job resource
    FakeJob<T> {
      job_token: root.ai.jobs.JobToken {
        provider: self.name(),
        id: options.idempotency_key ?? "fake-job",
        sealed: self.inner.output,
      },
      response: self.inner.generate<T>(task.with_provider(self.inner)) catch (e) {
        _ => throw baml.errors.UnknownError.with_message<never>(e, "fake background generation failed"),
      },
      cancelled: false,
    }
  }

  function resume_job<T>(
    self,
    token: root.ai.jobs.JobToken,
  ) -> root.ai.jobs.Job<T> throws baml.errors.UnknownError {
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
      root.ai.jobs.BackgroundOptions { idempotency_key: token.id },
    )
  }
}
