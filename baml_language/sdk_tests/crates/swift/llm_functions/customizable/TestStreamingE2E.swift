// Streaming end-to-end over the replay server — port of
// python_pydantic2's `test_streaming_e2e.py`. Every case runs against
// checked-in SSE recordings; no LLM keys.
//
// `stream_e2e_extract_stream` is the flat SDK streaming projection:
// an ordinary function returning `BamlStream<Partial, Final>`. `next()`
// yields `.value(partial)` until the engine's `ai.stream.Done` sentinel
// surfaces as `.finished` (a partial can legitimately be nil, hence
// the enum rather than Optional).
import XCTest
import Baml
import BamlBridge

final class TestStreamingE2E: XCTestCase {
    func test_streaming_e2e_stream() throws {
        try ReplayHarness.with(recording: "replay_extract_string") {
            let stream = try Baml.lorem.stream_e2e_extract_stream(text: "ignored-by-replay-server")
            var results = 0
            loop: while true {
                switch try stream.next() {
                case .finished:
                    break loop
                case .value:
                    results += 1
                    if results >= 10_000 {
                        XCTFail("stream never finished")
                        break
                    }
                }
            }
            XCTAssertGreaterThanOrEqual(results, 10)
            let final: String = try stream.final()
            XCTAssertFalse(final.isEmpty)
        }
    }

    func test_streaming_e2e_stream_async() async throws {
        try await ReplayHarness.withAsync(recording: "replay_extract_string") {
            let stream = try await Baml.lorem.stream_e2e_extract_stream_async(
                text: "ignored-by-replay-server"
            )
            var results = 0
            loop: while true {
                switch try await stream.nextAsync() {
                case .finished:
                    break loop
                case .value:
                    results += 1
                    if results >= 10_000 {
                        XCTFail("stream never finished")
                        break
                    }
                }
            }
            XCTAssertGreaterThanOrEqual(results, 10)
            let final: String = try await stream.finalAsync()
            XCTAssertFalse(final.isEmpty)
        }
    }

    func test_streaming_e2e_stream_collect_in_baml() throws {
        try ReplayHarness.with(recording: "replay_extract_string") {
            let result = try Baml.lorem.stream_e2e_collect(text: "ignored-by-replay-server")
            XCTAssertGreaterThanOrEqual(result.next_calls.count, 10)
            XCTAssertFalse(result.final_call.isEmpty)
        }
    }

    func test_streaming_e2e_stream_doc() throws {
        try ReplayHarness.with(recording: "replay_extract_doc") {
            let stream = try Baml.lorem.stream_e2e_extract_doc_stream(
                text: "ignored-by-replay-server"
            )
            var results = 0
            loop: while true {
                switch try stream.next() {
                case .finished:
                    break loop
                case .value(let partial):
                    results += 1
                    // Python asserts hasattr(v, "title"); the Swift
                    // analog is the typed partial's field access.
                    if let partial { _ = partial.title }
                    if results >= 10_000 {
                        XCTFail("stream never finished")
                        break
                    }
                }
            }
            XCTAssertGreaterThanOrEqual(results, 10)
            _ = try stream.final()
        }
    }

    func test_streaming_e2e_stream_doc_async() async throws {
        try await ReplayHarness.withAsync(recording: "replay_extract_doc") {
            let stream = try await Baml.lorem.stream_e2e_extract_doc_stream_async(
                text: "ignored-by-replay-server"
            )
            var results = 0
            loop: while true {
                switch try await stream.nextAsync() {
                case .finished:
                    break loop
                case .value:
                    results += 1
                    if results >= 10_000 {
                        XCTFail("stream never finished")
                        break
                    }
                }
            }
            XCTAssertGreaterThanOrEqual(results, 10)
            _ = try await stream.finalAsync()
        }
    }

    func test_streaming_e2e_stream_doc_collect_in_baml() throws {
        try ReplayHarness.with(recording: "replay_extract_doc") {
            let result = try Baml.lorem.stream_e2e_collect_doc(text: "ignored-by-replay-server")
            XCTAssertFalse(result.title.isEmpty)
        }
    }
}
