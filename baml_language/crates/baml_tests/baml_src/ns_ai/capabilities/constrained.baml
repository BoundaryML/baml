class ChoiceConstraint {
  values: string[],
}

interface Constrained requires Provider {
  function generate_constrained<T>(
    self,
    request: Request<T>,
    constraint: ChoiceConstraint,
  ) -> Response<T> throws baml.errors.CallError | baml.errors.UnknownError
}

function run_constrained_with_meta<T>(
  request: Request<T>,
  constraint: ChoiceConstraint,
) -> Response<T> throws baml.errors.CallError | baml.errors.UnknownError {
  match (request.provider) {
    let provider: Constrained => provider.generate_constrained<T>(request, constraint),
    _ => throw baml.errors.Unsupported {
      message: "provider does not support constrained decoding: " + request.provider_name(),
    },
  }
}

function run_constrained<T>(
  request: Request<T>,
  constraint: ChoiceConstraint,
) -> T throws baml.errors.CallError | baml.errors.UnknownError {
  run_constrained_with_meta<T>(request, constraint).value
}
