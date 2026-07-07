// Realtime usage shapes (scenarios 22, 23, 26), moved out of the stdlib.

// 22/26 — the `.live` companion: negotiate `Realtime` and hand in the channel.
function example_voice_chat(
    p: baml.ai.Provider,
    system: string,
    io: baml.ai.Channel,
) -> baml.ai.Transcript throws baml.errors.RealtimeError | baml.errors.UnknownError {
    match (p) {
        let r: baml.ai.Realtime => r.run(system, io),
        _ => throw baml.errors.Unsupported { message: "client is not realtime" },
    }
}

// 23 — barge-in: negotiate `LiveControl` on top of a live session.
function example_barge_in(
    p: baml.ai.Provider,
    io: baml.ai.Channel,
    played_ms: int,
) -> null throws baml.errors.RealtimeError | baml.errors.UnknownError {
    match (p) {
        let lc: baml.ai.LiveControl => lc.truncate(io, played_ms),
        _ => throw baml.errors.Unsupported { message: "client has no live control" },
    }
}
