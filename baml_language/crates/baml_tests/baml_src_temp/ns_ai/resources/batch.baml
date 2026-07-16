class BatchOptions {
  idempotency_key: string?,
}

interface Batch<T> requires Resource {
  function status(self) -> JobStatus throws never
  function results(self) -> Response<T>[] throws baml.errors.UnknownError
  function cancel(self) -> null throws baml.errors.UnknownError
}

interface BatchProvider requires Provider {
  function submit_batch<T>(self, tasks: Task<T>[], options: BatchOptions) -> Batch<T>
    throws baml.errors.UnknownError
}

class FakeBatch<T> {
  responses: Response<T>[],
  cancelled: bool,
  implements Batch<T> {
    function status(self) -> JobStatus throws never {
      if (self.cancelled) { JobStatus.Cancelled } else { JobStatus.Complete }
    }
    function results(self) -> Response<T>[] throws never { self.responses }
    function cancel(self) -> null throws never { self.cancelled = true; null }
  }
  implements Resource {
    function cleanup(self) -> null throws never { self.cancel() }
  }
}

class FakeBatchProvider {
  inner: FakeProvider,
  implements Provider {
    function name(self) -> string throws never { "fake-batch" }
    function render_shorthand(self) -> string throws never { self.inner.render_shorthand() }
  }
  implements BatchProvider {
    function submit_batch<T>(self, tasks: Task<T>[], options: BatchOptions) -> Batch<T>
        throws baml.errors.UnknownError {
      let responses: Response<T>[] = [];
      for (let task in tasks) {
        let response = self.inner.generate<T>(task.with_provider(self.inner)) catch (e) {
          _ => throw baml.errors.UnknownError {
            data: e,
            message: ["fake batch generation failed"],
          },
        };
        responses.push(response);
      }
      FakeBatch<T> { responses: responses, cancelled: false }
    }
  }
}
