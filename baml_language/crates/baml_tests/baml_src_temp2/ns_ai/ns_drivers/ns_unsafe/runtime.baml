// Runtime-negotiated escape hatches. With D-001, the executable reference
// already erases P; this namespace preserves the proposed call-site signal.
function drive<T>(task: root.ai.Task<T>) -> T
    throws baml.errors.CallError | baml.errors.UnknownError {
  root.ai.drivers.drive<T>(task)
}

function generate<T>(task: root.ai.Task<T>) -> T
    throws baml.errors.CallError | baml.errors.UnknownError {
  root.ai.drivers.generate<T>(task)
}

function generate_with_meta<T>(task: root.ai.Task<T>) -> root.ai.Response<T>
    throws baml.errors.CallError | baml.errors.UnknownError {
  root.ai.drivers.generate_with_meta<T>(task)
}
