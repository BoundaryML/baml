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

    private func checkBounded(_ count: Int) throws {
        guard count < 10_000 else {
            XCTFail("stream never finished")
            throw StreamNeverFinished()
        }
    }

    private func drain<Value>(_ stream: BamlStream<Value>) throws -> [Value] {
        var partials: [Value] = []
        while case .value(let partial) = try stream.nextSync() {
            partials.append(partial)
            try checkBounded(partials.count)
        }
        return partials
    }

    private func drainAsync<Value>(_ stream: BamlStream<Value>) async throws -> [Value] {
        var partials: [Value] = []
        while case .value(let partial) = try await stream.next() {
            partials.append(partial)
            try checkBounded(partials.count)
        }
        return partials
    }

    private func assertItemsPartials(_ partials: [[Item]]) {
        XCTAssertTrue(itemsSlot._bamlEqual(partials.first ?? [], [("a", 1)]))
        for partial in partials {
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
        let stream = try Baml.tuple_streams.stream_tuple_items_stream_sync(text: "x")
        assertItemsPartials(try drain(stream))
        XCTAssertTrue(itemsSlot._bamlEqual(try stream.finalSync(), items))
        XCTAssertTrue(itemsSlot._bamlEqual(try Baml.tuple_streams.stream_tuple_items_sync(text: "x"), items))

        let asyncStream = try await Baml.tuple_streams.stream_tuple_items_stream(text: "x")
        assertItemsPartials(try await drainAsync(asyncStream))
        let final = try await asyncStream.final()
        XCTAssertTrue(itemsSlot._bamlEqual(final, items))
    }

    func test_tuples_stream_final() async throws {
        let stream = try Baml.tuple_streams.stream_tuple_final_stream_sync(text: "x")
        assertFinalPartials(try drain(stream))
        XCTAssertTrue(finalSlot._bamlEqual(try stream.finalSync(), (1, "a")))
        XCTAssertTrue(finalSlot._bamlEqual(try Baml.tuple_streams.stream_tuple_final_sync(text: "x"), (1, "a")))

        let asyncStream = try await Baml.tuple_streams.stream_tuple_final_stream(text: "x")
        assertFinalPartials(try await drainAsync(asyncStream))
        let final = try await asyncStream.final()
        XCTAssertTrue(finalSlot._bamlEqual(final, (1, "a")))
    }
}
