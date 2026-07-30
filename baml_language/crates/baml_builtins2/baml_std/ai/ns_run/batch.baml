// A batch consumes the task collection as a whole, so it uses Runner<Task<T>[]>
// directly rather than pretending one Task<T> owns the plural lifecycle.
class Batch<T> {
    provider: ai.Provider,
    options: ai.jobs.BatchOptions,

    function new(
        provider: ai.Provider,
        idempotency_key: string? = null,
    ) -> Batch<T> throws never {
        Batch<T> {
            provider: provider,
            options: ai.jobs.BatchOptions { idempotency_key: idempotency_key },
        }
    }

    implements ai.Runner<ai.Task<T>[]> {
        type Output = ai.jobs.Batch<T>
        type Error = ai.Failure | baml.errors.UnknownError

        function run(
            self,
            tasks: ai.Task<T>[],
        ) -> ai.jobs.Batch<T> throws ai.Failure | baml.errors.UnknownError {
            //# Select a batch-capable provider for the task collection
            match (self.provider) {
                let batch: ai.jobs.BatchProvider => {
                    //## Submit all tasks as one provider-owned batch resource
                    batch.submit_batch<T>(tasks, self.options)
                },
                _ => {
                    //## Reject providers that cannot submit batches
                    throw baml.errors.Unsupported {
          message: "provider cannot submit batches",
        }
                },
            }
        }
    }
}
