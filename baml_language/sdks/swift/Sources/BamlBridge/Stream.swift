import Foundation

/// One `next()` poll result: a partial value, or the engine's
/// end-of-stream sentinel (`baml.stream.StreamFinished`). Distinct from
/// `Partial?` because a legitimate partial can itself be null.
public enum BamlStreamNext<Partial: BamlDecodable> {
    case value(Partial)
    case finished
}

extension BamlStreamNext: Sendable where Partial: Sendable {}
extension BamlStreamNext: Equatable where Partial: Equatable {}

/// A live BAML stream (`baml.llm.Stream<Partial, Final>`), handle-backed.
///
/// The engine holds the stream state; the wire carries an
/// `ADT_TAGGED_HEAP_HANDLE` whose table row remembers the receiver's
/// type. `next`/`final` reuse the ordinary call path with the handle as
/// the `self` receiver — the exact mechanism of Python's `_stream.py`
/// (no dedicated native stream API). A reference type on purpose:
/// consuming `next()` advances shared engine-side state, like `File`'s
/// cursor.
public final class BamlStream<Partial: BamlDecodable, Final: BamlDecodable>: @unchecked Sendable {
    private static var nextFQN: String { "baml.llm.Stream.next" }
    private static var finalFQN: String { "baml.llm.Stream.final" }
    private static var finishedFQN: String { "baml.stream.StreamFinished" }

    public let handle: BamlHandle

    public init(handle: BamlHandle) {
        self.handle = handle
    }

    public func next() throws -> BamlStreamNext<Partial> {
        try Self.interpretNext(
            BamlRuntime.shared.callRawSync(Self.nextFQN, args: [("self", handle)])
        )
    }

    public func nextAsync() async throws -> BamlStreamNext<Partial> {
        try Self.interpretNext(
            await BamlRuntime.shared.callRaw(Self.nextFQN, args: [("self", handle)])
        )
    }

    public func final() throws -> Final {
        try BamlRuntime.shared.callSync(Self.finalFQN, args: [("self", handle)])
    }

    public func finalAsync() async throws -> Final {
        try await BamlRuntime.shared.call(Self.finalFQN, args: [("self", handle)])
    }

    private static func interpretNext(_ raw: BamlOutboundValue) throws -> BamlStreamNext<Partial> {
        if raw.wireClassFQN() == finishedFQN {
            return .finished
        }
        return .value(try Partial._bamlDecode(raw))
    }
}

extension BamlStream: BamlEncodable {
    /// A stream argument rides as its bare tagged handle (Python lifts
    /// `BamlStream` to the inner `BamlPyHandle` the same way) — never a
    /// class-value wrapper.
    public func _bamlEncode() -> BamlInboundValue {
        handle._bamlEncode()
    }
}

extension BamlStream: BamlDecodable {
    public static func _bamlDecode(_ value: BamlOutboundValue) throws -> BamlStream<Partial, Final> {
        let handle = try BamlHandle._bamlDecode(value)
        guard handle.handleType == .adtTaggedHeapHandle else {
            throw BamlDecodeError.typeMismatch(
                expected: "stream (tagged heap handle)",
                got: "handle type \(handle.handleType)"
            )
        }
        return BamlStream(handle: handle)
    }
}
