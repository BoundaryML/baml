// A batch consumes the task collection as a whole, so it uses Runner<Task<T>[]>
// directly rather than pretending one Task<T> owns the plural lifecycle.
class Batch<T> {
    provider: root.ai.Provider,
    options: root.ai.BatchOptions,

    function new(
        provider: root.ai.Provider,
        idempotency_key: string? = null,
    ) -> Batch<T> throws never {
        Batch<T> {
            provider: provider,
            options: root.ai.BatchOptions { idempotency_key: idempotency_key },
        }
    }

    implements root.ai.Runner<root.ai.Task<T>[]> {
        type Output = root.ai.Batch<T>
        type Error = baml.errors.UnknownError | baml.errors.Unsupported

        function run(
            self,
            tasks: root.ai.Task<T>[],
        ) -> root.ai.Batch<T> throws baml.errors.UnknownError | baml.errors.Unsupported {
            //# Select a batch-capable provider for the task collection
            match (self.provider) {
                let batch: root.ai.BatchProvider => {
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
