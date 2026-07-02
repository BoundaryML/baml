// Harness usage (scenarios 37-42): drive a harness via the Realtime capability. The provider
// classes (ClaudeCode, PiAgent) live in stdlib ns_ai/harness.baml.

// 37 — configure + drive a harness (live channel handed in at the call).
function fix_bug(
    h: baml.ai.Provider,
    task: string,
    io: baml.ai.Channel,
) -> baml.ai.Transcript throws baml.errors.RealtimeError | baml.errors.UnknownError {
    match (h) {
        let r: baml.ai.Realtime => r.run("Fix the bug: " + task, io),
        _ => throw baml.errors.Unsupported { message: "client is not a drivable harness" },
    }
}

// 42 — one abstraction over many runtimes: ClaudeCode and PiAgent are both `Realtime`.
function drive_any(h: baml.ai.Provider) -> string throws never {
    match (h) {
        let r: baml.ai.Realtime => "drivable harness",
        _ => "not a harness",
    }
}
