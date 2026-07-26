// Background submission is unary from the task's point of view. The options
// configure the execution strategy; the returned Job<T> owns the remote
// lifecycle after submission.
class Background<T> {
    options: root.ai.BackgroundOptions,

    function new(idempotency_key: string? = null) -> Background<T> throws never {
        Background<T> { options: root.ai.BackgroundOptions { idempotency_key: idempotency_key } }
    }

    implements root.ai.Runner<root.ai.Task<T>> {
        type Output = root.ai.Job<T>
        type Error = baml.errors.UnknownError | baml.errors.Unsupported

        function run(
            self,
            task: root.ai.Task<T>,
        ) -> root.ai.Job<T> throws baml.errors.UnknownError | baml.errors.Unsupported {
            match (task.provider) {
                let provider: root.ai.BackgroundProvider => provider.submit<T>(task, self.options),
                _ => {
                    throw baml.errors.Unsupported {
          message: "provider cannot submit background work: " + task.provider_name(),
        }
                },
            }
        }
    }
}
