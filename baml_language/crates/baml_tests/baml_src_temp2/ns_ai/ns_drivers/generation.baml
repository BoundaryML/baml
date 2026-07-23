function complete_with_meta<T>(task: root.ai.Task<T>) -> root.ai.Response<T>
    throws root.ai.CallError | baml.errors.UnknownError {
  match (task.provider) {
    let provider: root.ai.CompletionProvider => provider.complete<T>(task),
    _ => throw baml.errors.Unsupported {
      message: "provider cannot complete a task: " + task.provider_name(),
    },
  }
}

function complete<T>(task: root.ai.Task<T>) -> T
    throws root.ai.CallError | baml.errors.UnknownError {
  complete_with_meta<T>(task).value
}

function generate_with_meta<T>(task: root.ai.Task<T>) -> root.ai.Response<T>
    throws root.ai.CallError | baml.errors.UnknownError {
  match (task.provider) {
    let provider: root.ai.GenerationProvider => provider.generate<T>(task),
    _ => throw baml.errors.Unsupported {
      message: "provider cannot perform one generation: " + task.provider_name(),
    },
  }
}

function generate<T>(task: root.ai.Task<T>) -> T
    throws root.ai.CallError | baml.errors.UnknownError {
  generate_with_meta<T>(task).value
}
