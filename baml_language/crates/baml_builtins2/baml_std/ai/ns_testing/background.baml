class FakeJob<T> {
    job_token: ai.jobs.JobToken,
    response: ai.ResponseWithMetadata<T>,
    cancelled: bool,

    // function cleanup(self) -> void throws never { self.cancel(); }

    implements ai.jobs.Job<T> {
        function status(self) -> ai.jobs.JobStatus throws never {
            if (self.cancelled) {
                ai.jobs.JobStatus.Cancelled
            } else {
                ai.jobs.JobStatus.Complete
            }
        }

        function poll(self) -> ai.ResponseWithMetadata<T>? throws never {
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

        function token(self) -> ai.jobs.JobToken throws never {
            self.job_token
        }
    }

    implements ai.Resource {}
}

class FakeBackgroundProvider {
    inner: FakeProvider,
}

implements ai.Provider for FakeBackgroundProvider {
  function name(self) -> string throws never { "fake-background" }
  function render_shorthand(self) -> string throws never { self.inner.render_shorthand() }
}

implements ai.jobs.BackgroundProvider for FakeBackgroundProvider {
  function submit<T>(
    self,
    task: ai.Task<T>,
    options: ai.jobs.BackgroundOptions,
  ) -> ai.jobs.Job<T> throws baml.errors.UnknownError {
    //# Execute immediately but expose the result through a durable job resource
    FakeJob<T> {
      job_token: ai.jobs.JobToken {
        provider: self.name(),
        id: options.idempotency_key ?? "fake-job",
        sealed: self.inner.output,
      },
      response: ai.internal._run_agent_to_response<T>(
        task.with_provider(self.inner),
      ) catch (e) {
        _ => throw baml.errors.UnknownError.with_message<never>(e, "fake background Agent failed"),
      },
      cancelled: false,
    }
  }

  function resume_job<T>(
    self,
    token: ai.jobs.JobToken,
  ) -> ai.jobs.Job<T> throws baml.errors.UnknownError {
    //# Reconstruct a task from the saved job token
    let task = ai.task<T>(
      self.inner,
      (ctx: baml.llm.Context) -> baml.llm.PromptAst {
        baml.llm.assemble_prompt_ast(["resume"], [])
      },
    );
    //# Resubmit with the original durable job identity
    self.submit<T>(
      task,
      ai.jobs.BackgroundOptions { idempotency_key: token.id },
    )
  }
}
