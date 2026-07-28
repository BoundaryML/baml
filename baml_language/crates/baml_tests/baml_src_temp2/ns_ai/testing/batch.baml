class FakeBatch<T> {
    responses: ResponseWithMetadata<T>[],
    cancelled: bool,

    implements Batch<T> {
        function status(self) -> JobStatus throws never {
            if (self.cancelled) {
                JobStatus.Cancelled
            } else {
                JobStatus.Complete
            }
        }

        function results(self) -> ResponseWithMetadata<T>[] throws never {
            self.responses
        }

        function cancel(self) -> null throws never {
            self.cancelled = true;
            null
        }
    }

    implements Resource {}
}

class FakeBatchProvider {
    inner: FakeProvider,

    implements Provider {
        function name(self) -> string throws never {
            "fake-batch"
        }
        function render_shorthand(self) -> string throws never {
            self.inner.render_shorthand()
        }
    }

    implements BatchProvider {
        function submit_batch<T>(
            self,
            tasks: Task<T>[],
            options: BatchOptions,
        ) -> Batch<T> throws baml.errors.UnknownError {
            let responses: ResponseWithMetadata<T>[] = [];
            //# Execute every task in the batch
            for (let task in tasks) {
                //## Generate one batch result
                let response = self.inner.generate<T>(task.with_provider(self.inner)) catch (e) {
                    _ => {
                        throw baml.errors.UnknownError {
            data: e,
            message: ["fake batch generation failed"],
          }
                    },
                };
                responses.push(response);
            }
            FakeBatch<T> { responses: responses, cancelled: false }
        }
    }
}
