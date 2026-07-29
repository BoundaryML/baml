class FakeBatch<T> {
    responses: root.ai.ResponseWithMetadata<T>[],
    cancelled: bool,

    implements root.ai.jobs.Batch<T> {
        function status(self) -> root.ai.jobs.JobStatus throws never {
            if (self.cancelled) {
                root.ai.jobs.JobStatus.Cancelled
            } else {
                root.ai.jobs.JobStatus.Complete
            }
        }

        function results(self) -> root.ai.ResponseWithMetadata<T>[] throws never {
            self.responses
        }

        function cancel(self) -> null throws never {
            self.cancelled = true;
            null
        }
    }

    implements root.ai.Resource {}
}

class FakeBatchProvider {
    inner: FakeProvider,

    implements root.ai.Provider {
        function name(self) -> string throws never {
            "fake-batch"
        }
        function render_shorthand(self) -> string throws never {
            self.inner.render_shorthand()
        }
    }

    implements root.ai.jobs.BatchProvider {
        function submit_batch<T>(
            self,
            tasks: root.ai.Task<T>[],
            options: root.ai.jobs.BatchOptions,
        ) -> root.ai.jobs.Batch<T> throws baml.errors.UnknownError {
            let responses: root.ai.ResponseWithMetadata<T>[] = [];
            //# Execute every task in the batch
            for (let task in tasks) {
                //## Run one batch task through the normal Agent lifecycle
                let response = root.ai.internal._run_agent_to_response<T>(task.with_provider(self.inner)) catch (e) {
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
