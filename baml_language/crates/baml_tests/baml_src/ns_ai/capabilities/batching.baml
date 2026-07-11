class Batch<T> { id: string, owner: string }
interface Batching requires Provider {
  function submit_batch<T>(
    self,
    requests: Request<T>[],
    key_of: (Request<T>, int) -> string throws never,
  ) -> Batch<T> throws baml.errors.UnknownError
}

function submit_batch<T>(
  provider: Provider,
  requests: Request<T>[],
  key_of: (Request<T>, int) -> string throws never,
) -> Batch<T> throws baml.errors.UnknownError {
  match (provider) {
    let batching: Batching => batching.submit_batch<T>(requests, key_of),
    _ => throw baml.errors.UnknownError { data: provider, message: ["provider cannot batch"] },
  }
}
