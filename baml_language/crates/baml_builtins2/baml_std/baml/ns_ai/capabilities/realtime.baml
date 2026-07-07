// The Realtime capability (scenarios 22–26). A realtime provider is a marker that
// implements `Realtime` directly — no degenerate `call` to fake (OQ1). The live
// `Channel` is a parameter of `run` (pass-in), handed in at the call, never a provider
// field. Live implementation: providers/openai_realtime.baml (over `baml.ws`).

/// The final record returned when a live session ends.
class Transcript {
    text: string,
}

/// A live, duplex I/O channel: the caller hands it in, the provider drives it.
interface Channel {
    function on(self, handler: (string) -> void) -> null throws never
    function send(self, data: string) -> null throws root.errors.Io
    function close(self) -> null throws never
}

/// A live, duplex interaction. Owns `run`; requires only the `Provider` marker.
interface Realtime requires root.ai.Provider {
    function run(self, prompt: string, io: Channel) -> Transcript
        throws root.errors.RealtimeError | root.errors.UnknownError
}

/// Barge-in / interruption / mid-session mutation over a live session (scenario 23).
interface LiveControl requires Realtime {
    function cancel(self, io: Channel) -> null
        throws root.errors.RealtimeError | root.errors.UnknownError
    function truncate(self, io: Channel, played_ms: int) -> null
        throws root.errors.RealtimeError | root.errors.UnknownError
}
