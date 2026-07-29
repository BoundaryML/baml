interface RealtimeProvider requires root.ai.Provider {
  function open_live<T>(self, task: root.ai.Task<T>, channel: Channel) -> LiveSession
    throws baml.errors.UnknownError
}

// Opening a raw provider session is a resource operation, not a managed
// execution lifecycle. Higher-level runners such as VoiceAgent build their
// event loops and application-tool policy on top of this primitive.
function open_live(
    task: root.ai.Task<null>,
    channel: Channel,
) -> LiveSession throws root.ai.Failure | baml.errors.UnknownError {
    //# Select a provider that can open a persistent realtime session
    match (task.provider) {
        let provider: RealtimeProvider => {
            //## Open the raw provider session without taking over its event loop
            provider.open_live<null>(task, channel)
        },
        _ => {
            //## Reject providers without realtime support
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
    tools: root.ai.tools.ToolInput[],
) -> LiveSession throws never {
    //# Wrap a raw live session with application-tool dispatch
    AutomaticToolLiveSession { inner: session, tools: root.ai.tools.normalize_tools(tools) }
}
