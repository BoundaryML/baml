// Keyless SSE replay — port of python_pydantic2's `replay_harness.py`.
//
// The replay server is itself BAML (ns_replay fixture): we call the
// generated `replay_serve_detached`, which binds 127.0.0.1:0, spawns
// the serving loop engine-side, and returns the address immediately —
// no host thread or addr-file dance needed (Python predates the
// detached variant). The StreamStub client resolves
// BAML_REPLAY_BASE_URL / BAML_REPLAY_API_KEY at call time, so setenv
// in-process redirects it; no real LLM key is ever required.
import Foundation
import Baml

enum ReplayHarness {
    static func recordingPath(_ name: String) -> String {
        // …/crates/swift/llm_functions/generated/Tests/BamlTests/<file>
        //   → up to sdk_tests, then fixtures/llm_functions/recordings.
        var url = URL(fileURLWithPath: #filePath)
        while url.lastPathComponent != "sdk_tests" && url.pathComponents.count > 1 {
            url.deleteLastPathComponent()
        }
        return url
            .appendingPathComponent("fixtures/llm_functions/recordings/\(name).snap.sse")
            .path
    }

    /// Run `body` with a live replay server; always shuts it down.
    static func with<T>(recording: String, _ body: () throws -> T) throws -> T {
        let addr = try Baml.replay.replay_serve_detached(
            recording_path: recordingPath(recording)
        )
        setenv("BAML_REPLAY_BASE_URL", "http://\(addr)", 1)
        setenv("BAML_REPLAY_API_KEY", "replay-test-key", 1)
        defer {
            shutdown(addr)
            unsetenv("BAML_REPLAY_BASE_URL")
            unsetenv("BAML_REPLAY_API_KEY")
        }
        return try body()
    }

    static func withAsync<T>(
        recording: String,
        _ body: () async throws -> T
    ) async throws -> T {
        let addr = try await Baml.replay.replay_serve_detached_async(
            recording_path: recordingPath(recording)
        )
        setenv("BAML_REPLAY_BASE_URL", "http://\(addr)", 1)
        setenv("BAML_REPLAY_API_KEY", "replay-test-key", 1)
        defer {
            shutdown(addr)
            unsetenv("BAML_REPLAY_BASE_URL")
            unsetenv("BAML_REPLAY_API_KEY")
        }
        return try await body()
    }

    private static func shutdown(_ addr: String) {
        guard let url = URL(string: "http://\(addr)/__replay__/shutdown") else { return }
        var request = URLRequest(url: url, timeoutInterval: 5)
        request.httpMethod = "POST"
        request.httpBody = Data()
        let done = DispatchSemaphore(value: 0)
        URLSession.shared.dataTask(with: request) { _, _, _ in done.signal() }.resume()
        _ = done.wait(timeout: .now() + 6)
    }
}
