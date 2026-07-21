// Background submission is unary from the task's point of view. The options
// configure the execution strategy; the returned Job<T> owns the remote
// lifecycle after submission.
class Background<T> {
  options: root.ai.BackgroundOptions,
}

implements<T> root.ai.Driver<root.ai.Task<T>> for Background<T> {
  type Output = root.ai.Job<T>
  type Error = baml.errors.UnknownError | baml.errors.Unsupported

  function drive(self, task: root.ai.Task<T>) -> root.ai.Job<T>
      throws baml.errors.UnknownError | baml.errors.Unsupported {
    root.ai.drivers.submit_background<T>(task, self.options)
  }
}

function background<T>(options: root.ai.BackgroundOptions) -> Background<T> throws never {
  Background<T> { options: options }
}
