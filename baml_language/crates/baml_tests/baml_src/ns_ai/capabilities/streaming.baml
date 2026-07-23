interface Streaming requires Provider {
  function stream<TPartial, T>(self, request: Request<T>) -> baml.llm.Stream<TPartial, T>
    throws baml.errors.StreamError | baml.errors.UnknownError
}

function stream<TPartial, T>(request: Request<T>) -> baml.llm.Stream<TPartial, T>
    throws baml.errors.StreamError | baml.errors.UnknownError {
  match (request.provider) {
    let streaming: Streaming => streaming.stream<TPartial, T>(request),
    _ => throw baml.errors.Unsupported {
      message: "provider cannot stream: " + request.provider_name(),
    },
  }
}
