// The harness and its policy configure how one task is submitted. HarnessRun<T>
// retains the terminal T, events, conversation, and resume token.
class Harness<T> {
  harness: root.ai.Harness,
  options: root.ai.HarnessOptions,
}

implements<T> root.ai.Driver<root.ai.Task<T>> for Harness<T> {
  type Output = root.ai.HarnessRun<T>
  type Error = baml.errors.UnknownError

  function drive(self, task: root.ai.Task<T>) -> root.ai.HarnessRun<T>
      throws baml.errors.UnknownError {
    root.ai.drivers.submit_harness<T>(self.harness, task, self.options)
  }
}

function harness<T>(
  harness: root.ai.Harness,
  options: root.ai.HarnessOptions,
) -> Harness<T> throws never {
  Harness<T> { harness: harness, options: options }
}
