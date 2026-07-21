// Streaming is another nominal strategy value. Both the final output and the
// compiler-projected partial type remain visible in the driver's Output.
class Stream<TPartial, T> {}

implements<TPartial, T> root.ai.Driver<root.ai.Task<T>> for Stream<TPartial, T> {
  type Output = baml.llm.Stream<TPartial, T>
  type Error = baml.errors.StreamError | baml.errors.UnknownError | baml.errors.Unsupported

  function drive(self, task: root.ai.Task<T>) -> baml.llm.Stream<TPartial, T>
      throws baml.errors.StreamError | baml.errors.UnknownError | baml.errors.Unsupported {
    root.ai.drivers.stream<TPartial, T>(task)
  }
}

function stream<TPartial, T>() -> Stream<TPartial, T> throws never {
  Stream<TPartial, T> {}
}
