// The Realtime capability + examples (scenarios 22–26). A realtime provider is a marker
// that implements `Realtime` directly — no degenerate `call` to fake (OQ1). The live
// `Channel` is a parameter of `run` (pass-in), handed in at the call, never a provider field.
//
// The duplex transport (WebSocket/WebRTC) is genuinely-missing host surface (plan P8), so
// `run` throws until it is wired; the capability shape, config-only client, pass-in channel,
// and negotiation are all real and compiling.

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

/// A realtime voice provider (scenario 22). Config-only fields; the channel is a `run`
/// parameter. `run` throws until the duplex transport (P8) is wired.
class OpenAiRealtime {
    voice: string,
    api_key: string,

    implements root.ai.Provider {}

    implements Realtime {
        function run(
            self,
            prompt: string,
            io: Channel,
        ) -> Transcript throws root.errors.RealtimeError | root.errors.UnknownError {
            throw root.errors.Unsupported { message: "realtime transport not yet wired (needs duplex host surface, plan P8)" }
        }
    }

    implements LiveControl {
        function cancel(
            self,
            io: Channel,
        ) -> null throws root.errors.RealtimeError | root.errors.UnknownError {
            throw root.errors.Unsupported { message: "realtime transport not yet wired" }
        }
        function truncate(
            self,
            io: Channel,
            played_ms: int,
        ) -> null throws root.errors.RealtimeError | root.errors.UnknownError {
            throw root.errors.Unsupported { message: "realtime transport not yet wired" }
        }
    }
}

// --- usage examples ---

// Scenario 22/26: the `.live` companion — negotiate `Realtime` and hand in the channel.
function example_voice_chat(
    p: root.ai.Provider,
    system: string,
    io: Channel,
) -> Transcript throws root.errors.RealtimeError | root.errors.UnknownError {
    match (p) {
        let r: Realtime => r.run(system, io),
        _ => throw root.errors.Unsupported { message: "client is not realtime" },
    }
}

// Scenario 23: barge-in — negotiate `LiveControl` on top of a live session.
function example_barge_in(
    p: root.ai.Provider,
    io: Channel,
    played_ms: int,
) -> null throws root.errors.RealtimeError | root.errors.UnknownError {
    match (p) {
        let lc: LiveControl => lc.truncate(io, played_ms),
        _ => throw root.errors.Unsupported { message: "client has no live control" },
    }
}
