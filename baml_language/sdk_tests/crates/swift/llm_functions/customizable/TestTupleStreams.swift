// Tuples as stream items, offline: `tuple_streams` scripts each function's
// client to replay fixed text deltas, so the generated stream companions run
// the ordinary partial/final parse with no replay server.
//
//   * `stream_tuple_items` -- `(string, int)[]`: partials are arrays of
//     `BamlTuple2`, `[("a", 1)]` first; the final value is
//     `[("a", 1), ("b", 2)]`.
//   * `stream_tuple_final` -- `(int, string)`: every partial is itself a
//     `BamlTuple2`, and so is the final value `(1, "a")`.
//
// Like `TestStreamingE2E.swift`, this needs the streaming projections that
// sdkgen_swift does not emit yet (see the `llm_functions later` row in
// crates/swift/src/lib.rs).
import XCTest
import Baml
import BamlBridge

final class TestTupleStreams: XCTestCase {
    typealias Item = BamlTuple2<String, Int>
    typealias Final = BamlTuple2<Int, String>

    private let items: [Item] = [BamlTuple2("a", 1), BamlTuple2("b", 2)]

    private func drain<Value>(_ stream: BamlStream<Value>) throws -> [Value] {
        var partials: [Value] = []
        while case .value(let partial) = try stream.next() {
            partials.append(partial)
            XCTAssertLessThan(partials.count, 10_000, "stream never finished")
        }
        return partials
    }

    private func drainAsync<Value>(_ stream: BamlStream<Value>) async throws -> [Value] {
        var partials: [Value] = []
        while case .value(let partial) = try await stream.nextAsync() {
            partials.append(partial)
            XCTAssertLessThan(partials.count, 10_000, "stream never finished")
        }
        return partials
    }

    private func assertItemsPartials(_ partials: [[Item]]) {
        XCTAssertEqual(partials.first, [BamlTuple2("a", 1)])
        for partial in partials {
            // Each partial is a prefix of the final value.
            XCTAssertEqual(partial, Array(items.prefix(partial.count)))
        }
    }

    private func assertFinalPartials(_ partials: [Final]) {
        XCTAssertFalse(partials.isEmpty)
        for partial in partials {
            XCTAssertEqual(partial.t0, 1)
            XCTAssertTrue("a".hasPrefix(partial.t1))
        }
        XCTAssertEqual(partials.last, BamlTuple2(1, "a"))
    }

    func test_tuples_stream_items() async throws {
        let stream = try Baml.tuple_streams.stream_tuple_items_stream(text: "x")
        assertItemsPartials(try drain(stream))
        XCTAssertEqual(try stream.final(), items)
        // The non-streaming call parses the same reply to the same value.
        XCTAssertEqual(try Baml.tuple_streams.stream_tuple_items(text: "x"), items)

        let asyncStream = try await Baml.tuple_streams.stream_tuple_items_stream_async(text: "x")
        assertItemsPartials(try await drainAsync(asyncStream))
        XCTAssertEqual(try await asyncStream.finalAsync(), items)
    }

    func test_tuples_stream_final() async throws {
        let stream = try Baml.tuple_streams.stream_tuple_final_stream(text: "x")
        assertFinalPartials(try drain(stream))
        XCTAssertEqual(try stream.final(), BamlTuple2(1, "a"))
        XCTAssertEqual(try Baml.tuple_streams.stream_tuple_final(text: "x"), BamlTuple2(1, "a"))

        let asyncStream = try await Baml.tuple_streams.stream_tuple_final_stream_async(text: "x")
        assertFinalPartials(try await drainAsync(asyncStream))
        XCTAssertEqual(try await asyncStream.finalAsync(), BamlTuple2(1, "a"))
    }
}
