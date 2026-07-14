function stream<TPartial, T>(task: root.ai.Task<T>) -> baml.llm.Stream<TPartial, T>
    throws baml.errors.StreamError | baml.errors.UnknownError | baml.errors.Unsupported {
  match (task.provider) {
    let provider: root.ai.StreamingProvider => provider.stream<TPartial, T>(task),
    _ => throw baml.errors.Unsupported {
      message: "provider cannot stream: " + task.provider_name(),
    },
  }
}

