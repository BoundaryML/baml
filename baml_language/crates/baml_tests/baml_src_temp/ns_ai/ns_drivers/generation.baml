function drive_with_meta<T>(task: root.ai.Task<T>) -> root.ai.Response<T>
    throws baml.errors.UnknownError | baml.errors.Unsupported {
  match (task.provider) {
    let provider: root.ai.DriveProvider => provider.drive<T>(task),
    _ => throw baml.errors.Unsupported {
      message: "provider cannot drive a task to a final value: " + task.provider_name(),
    },
  }
}

function drive<T>(task: root.ai.Task<T>) -> T
    throws baml.errors.UnknownError | baml.errors.Unsupported {
  drive_with_meta<T>(task).value
}

function generate<T>(task: root.ai.Task<T>) -> root.ai.Response<T>
    throws baml.errors.UnknownError | baml.errors.Unsupported {
  match (task.provider) {
    let provider: root.ai.GenerationProvider => provider.generate<T>(task),
    _ => throw baml.errors.Unsupported {
      message: "provider cannot perform one generation: " + task.provider_name(),
    },
  }
}
