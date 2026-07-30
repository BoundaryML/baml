// Background submission is unary from the task's point of view. The options
// configure the execution strategy; the returned Job<T> owns the remote
// lifecycle after submission.
class Background<T> {
    options: ai.jobs.BackgroundOptions,

    function new(idempotency_key: string? = null) -> Background<T> throws never {
        Background<T> {
            options: ai.jobs.BackgroundOptions { idempotency_key: idempotency_key },
        }
    }

    implements ai.Runner<ai.Task<T>> {
        type Output = ai.jobs.Job<T>
        type Error = ai.Failure | baml.errors.UnknownError

        function run(
            self,
            task: ai.Task<T>,
        ) -> ai.jobs.Job<T> throws ai.Failure | baml.errors.UnknownError {
            //# Select a background-capable provider
            match (task.provider) {
                let provider: ai.jobs.BackgroundProvider => {
                    //## Submit remote work and return its durable job resource
                    provider.submit<T>(task, self.options)
                },
                _ => {
                    //## Reject providers that cannot submit background work
                    throw baml.errors.Unsupported {
          message: "provider cannot submit background work: " + task.provider_name(),
        }
                },
            }
        }
    }
}
