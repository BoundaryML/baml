interface RealtimeProvider requires Provider {
  function open_live<T>(self, task: Task<T>, channel: Channel) -> LiveSession
    throws baml.errors.UnknownError
}

// Opening a raw provider session is a resource operation, not a managed
// execution lifecycle. Higher-level runners such as VoiceAgent build their
// event loops and application-tool policy on top of this primitive.
function open_live(
    task: Task<null>,
    channel: Channel,
) -> LiveSession throws baml.errors.UnknownError | baml.errors.Unsupported {
    match (task.provider) {
        let provider: RealtimeProvider => provider.open_live<null>(task, channel),
        _ => {
            throw baml.errors.Unsupported {
      message: "provider cannot open a realtime channel: " + task.provider_name(),
    }
        },
    }
}

// Automatic application-tool execution is opt-in for raw sessions. The
// provider session continues to own event ordering, call IDs, and result
// submission; this wrapper owns only function dispatch.
function with_automatic_tools(
    session: LiveSession,
    tools: ToolInput[],
) -> LiveSession throws never {
    AutomaticToolLiveSession { inner: session, tools: normalize_tools(tools) }
}
