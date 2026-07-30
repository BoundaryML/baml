class FakeBatch<T> {
    responses: ai.ResponseWithMetadata<T>[],
    cancelled: bool,

    implements ai.jobs.Batch<T> {
        function status(self) -> ai.jobs.JobStatus throws never {
            if (self.cancelled) {
                ai.jobs.JobStatus.Cancelled
            } else {
                ai.jobs.JobStatus.Complete
            }
        }

        function results(self) -> ai.ResponseWithMetadata<T>[] throws never {
            self.responses
        }

        function cancel(self) -> null throws never {
            self.cancelled = true;
            null
        }
    }

    implements ai.Resource {}
}

class FakeBatchProvider {
    inner: FakeProvider,

    implements ai.Provider {
        function name(self) -> string throws never {
            "fake-batch"
        }
        function render_shorthand(self) -> string throws never {
            self.inner.render_shorthand()
        }
    }

    implements ai.jobs.BatchProvider {
        function submit_batch<T>(
            self,
            tasks: ai.Task<T>[],
            options: ai.jobs.BatchOptions,
        ) -> ai.jobs.Batch<T> throws baml.errors.UnknownError {
            let responses: ai.ResponseWithMetadata<T>[] = [];
            //# Execute every task in the batch
            for (let task in tasks) {
                //## Run one batch task through the normal Agent lifecycle
                let response = ai.internal._run_agent_to_response<T>(task.with_provider(self.inner)) catch (e) {
                    _ => {
                        throw baml.errors.UnknownError.with_message<never>(e, "fake batch Agent failed")
                    },
                };
                responses.push(response);
            }
            FakeBatch<T> { responses: responses, cancelled: false }
        }
    }
}
