function open_live<T>(
  task: root.ai.Task<T>,
  channel: root.ai.Channel,
) -> root.ai.Live throws baml.errors.UnknownError | baml.errors.Unsupported {
  match (task.provider) {
    let provider: root.ai.RealtimeProvider => provider.open_live<T>(task, channel),
    _ => throw baml.errors.Unsupported {
      message: "provider cannot open a realtime channel: " + task.provider_name(),
    },
  }
}

