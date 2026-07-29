// Background submission is unary from the task's point of view. The options
// configure the execution strategy; the returned Job<T> owns the remote
// lifecycle after submission.
class Background<T> {
    options: root.ai.jobs.BackgroundOptions,

    function new(idempotency_key: string? = null) -> Background<T> throws never {
        Background<T> { options: root.ai.jobs.BackgroundOptions { idempotency_key: idempotency_key } }
    }

    implements root.ai.Runner<root.ai.Task<T>> {
        type Output = root.ai.jobs.Job<T>
        type Error = root.ai.Failure | baml.errors.UnknownError

        function run(
            self,
            task: root.ai.Task<T>,
        ) -> root.ai.jobs.Job<T> throws root.ai.Failure | baml.errors.UnknownError {
            //# Select a background-capable provider
            match (task.provider) {
                let provider: root.ai.jobs.BackgroundProvider => {
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
