// Streaming is another nominal strategy value. Both the final output and the
// compiler-projected partial type remain visible in the runner's Output. Like
// model output, it needs no separate state because the Task owns provider and
// request configuration.
class Stream<TPartial, T> {
    function new() -> Stream<TPartial, T> throws never {
        Stream<TPartial, T> {  }
    }

    implements root.ai.Runner<root.ai.Task<T>> {
        type Output = baml.llm.Stream<TPartial, T>
        type Error = root.ai.Failure | baml.errors.UnknownError

        function run(
            self,
            task: root.ai.Task<T>,
        ) -> baml.llm.Stream<TPartial, T> throws root.ai.Failure | baml.errors.UnknownError | baml.errors.Unsupported {
            //# Select streaming capability without draining the returned stream
            match (task.provider) {
                let provider: root.ai.StreamingProvider => {
                    //## Start the typed stream
                    provider.stream<TPartial, T>(task)
                },
                _ => {
                    //## Reject providers without streaming support
                    throw baml.errors.Unsupported {
          message: "provider cannot stream: " + task.provider_name(),
        }
                },
            }
        }
    }
}
