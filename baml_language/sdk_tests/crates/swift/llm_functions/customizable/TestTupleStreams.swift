// Tuples as stream items, offline: `tuple_streams` scripts each function's
// client to replay fixed text deltas, so the generated stream companions run
// the ordinary partial/final parse with no replay server.
//
//   * `stream_tuple_items` -- `(string, int)[]`: partials are arrays of
//     native Swift tuples, `[("a", 1)]` first; the final value is
//     `[("a", 1), ("b", 2)]`.
//   * `stream_tuple_final` -- `(int, string)`: every partial is itself a
//     native tuple, and so is the final value `(1, "a")`.
//
// A native tuple is not `Equatable`; values compare through their slots.
//
// Like `TestStreamingE2E.swift`, this needs the streaming projections that
// sdkgen_swift does not emit yet (see the `llm_functions later` row in
// crates/swift/src/lib.rs).
import XCTest
import Baml
import BamlBridge

final class TestTupleStreams: XCTestCase {
    typealias Item = (String, Int)
    typealias Final = (Int, String)

    private let items: [Item] = [("a", 1), ("b", 2)]
    private let itemsSlot = BamlSlots.list(BamlSlots.tuple(BamlSlot<String>.own, BamlSlot<Int>.own))
    private let finalSlot = BamlSlots.tuple(BamlSlot<Int>.own, BamlSlot<String>.own)

    private struct StreamNeverFinished: Error {}

    /// Fails the test and stops the drain (by throwing) once a stream has
    /// produced implausibly many partials, so a stuck stream cannot hang.
    private func checkBounded(_ count: Int) throws {
        guard count < 10_000 else {
            XCTFail("stream never finished")
            throw StreamNeverFinished()
        }
    }

    private func drain<Value>(_ stream: BamlStream<Value>) throws -> [Value] {
        var partials: [Value] = []
        while case .value(let partial) = try stream.next() {
            partials.append(partial)
            try checkBounded(partials.count)
        }
        return partials
    }

    private func drainAsync<Value>(_ stream: BamlStream<Value>) async throws -> [Value] {
        var partials: [Value] = []
        while case .value(let partial) = try await stream.nextAsync() {
            partials.append(partial)
            try checkBounded(partials.count)
        }
        return partials
    }

    private func assertItemsPartials(_ partials: [[Item]]) {
        XCTAssertTrue(itemsSlot._bamlEqual(partials.first ?? [], [("a", 1)]))
        for partial in partials {
            // Each partial is a prefix of the final value.
            XCTAssertTrue(itemsSlot._bamlEqual(partial, Array(items.prefix(partial.count))))
        }
    }

    private func assertFinalPartials(_ partials: [Final]) {
        XCTAssertFalse(partials.isEmpty)
        for partial in partials {
            XCTAssertEqual(partial.0, 1)
            XCTAssertTrue("a".hasPrefix(partial.1))
        }
        XCTAssertTrue(partials.last.map { $0 == (1, "a") } ?? false)
    }

    func test_tuples_stream_items() async throws {
        let stream = try Baml.tuple_streams.stream_tuple_items_stream(text: "x")
        assertItemsPartials(try drain(stream))
        XCTAssertTrue(itemsSlot._bamlEqual(try stream.final(), items))
        // The non-streaming call parses the same reply to the same value.
        XCTAssertTrue(itemsSlot._bamlEqual(try Baml.tuple_streams.stream_tuple_items(text: "x"), items))

        let asyncStream = try await Baml.tuple_streams.stream_tuple_items_stream_async(text: "x")
        assertItemsPartials(try await drainAsync(asyncStream))
        let final = try await asyncStream.finalAsync()
        XCTAssertTrue(itemsSlot._bamlEqual(final, items))
    }

    func test_tuples_stream_final() async throws {
        let stream = try Baml.tuple_streams.stream_tuple_final_stream(text: "x")
        assertFinalPartials(try drain(stream))
        XCTAssertTrue(finalSlot._bamlEqual(try stream.final(), (1, "a")))
        XCTAssertTrue(finalSlot._bamlEqual(try Baml.tuple_streams.stream_tuple_final(text: "x"), (1, "a")))

        let asyncStream = try await Baml.tuple_streams.stream_tuple_final_stream_async(text: "x")
        assertFinalPartials(try await drainAsync(asyncStream))
        let final = try await asyncStream.finalAsync()
        XCTAssertTrue(finalSlot._bamlEqual(final, (1, "a")))
    }
}
