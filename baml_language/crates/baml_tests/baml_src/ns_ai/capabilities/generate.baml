interface Generate requires Provider {
  function generate<T>(self, request: Request<T>) -> Response<T>
    throws baml.errors.CallError | baml.errors.UnknownError
}

function run_with_meta<T>(request: Request<T>) -> Response<T>
    throws baml.errors.CallError | baml.errors.UnknownError {
  match (request.provider) {
    let generate: Generate => generate.generate<T>(request),
    _ => throw baml.errors.Unsupported {
      message: "provider cannot generate: " + request.provider_name(),
    },
  }
}

function run<T>(request: Request<T>) -> T
    throws baml.errors.CallError | baml.errors.UnknownError {
  run_with_meta<T>(request).value
}
