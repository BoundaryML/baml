// Streaming is another nominal strategy value. Both the final output and the
// compiler-projected partial type remain visible in the runner's Output. Like
// generation, it needs no separate state because the Task owns provider and
// request configuration.
class Stream<TPartial, T> {
  function new() -> Stream<TPartial, T> throws never { Stream<TPartial, T> {} }

  implements root.ai.Runner<root.ai.Task<T>> {
    type Output = baml.llm.Stream<TPartial, T>
    type Error = root.ai.StreamError | baml.errors.UnknownError | baml.errors.Unsupported

    function run(self, task: root.ai.Task<T>) -> baml.llm.Stream<TPartial, T>
        throws root.ai.StreamError | baml.errors.UnknownError | baml.errors.Unsupported {
      root.ai.drivers.stream<TPartial, T>(task)
    }
  }
}
