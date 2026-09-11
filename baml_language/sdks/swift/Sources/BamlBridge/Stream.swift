import Foundation

/// One `next()` poll result: a partial value, or the engine's
/// end-of-stream sentinel (`ai.stream.Done`). Distinct from
/// `Partial?` because a legitimate partial can itself be null.
public enum BamlStreamNext<Partial: BamlDecodable> {
    case value(Partial)
    case finished
}

extension BamlStreamNext: Sendable where Partial: Sendable {}
extension BamlStreamNext: Equatable where Partial: Equatable {}

/// A live BAML stream (`ai.stream.Stream<Partial, Final>`), handle-backed.
///
/// The engine holds the stream state; the wire carries an
/// `ADT_TAGGED_HEAP_HANDLE` whose table row remembers the receiver's
/// type. `next`/`final` reuse the ordinary call path with the handle as
/// the `self` receiver — the exact mechanism of Python's `_stream.py`
/// (no dedicated native stream API). A reference type on purpose:
/// consuming `next()` advances shared engine-side state, like `File`'s
/// cursor.
public final class BamlStream<Partial: BamlDecodable, Final: BamlDecodable>: @unchecked Sendable {
    private static var doneFQN: String { "ai.stream.Done" }

    public let handle: BamlHandle
    /// Exact root class identity carried by the outbound tagged handle.
    public let bamlClassFQN: String

    public init(handle: BamlHandle) {
        guard let classFQN = handle.classFQN, !classFQN.isEmpty else {
            preconditionFailure("tagged stream handle is missing its carried BAML class identity")
        }
        self.handle = handle
        self.bamlClassFQN = classFQN
    }

    var nextFQN: String { "\(bamlClassFQN).next" }
    var finalFQN: String { "\(bamlClassFQN).final" }

    public func next() throws -> BamlStreamNext<Partial> {
        try Self.interpretNext(
            handle.runtime.callRawSync(nextFQN, args: [("self", handle)])
        )
    }

    public func nextAsync() async throws -> BamlStreamNext<Partial> {
        try Self.interpretNext(
            await handle.runtime.callRaw(nextFQN, args: [("self", handle)])
        )
    }

    public func final() throws -> Final {
        try handle.runtime.callSync(finalFQN, args: [("self", handle)])
    }

    public func finalAsync() async throws -> Final {
        try await handle.runtime.call(finalFQN, args: [("self", handle)])
    }

    private static func interpretNext(_ raw: BamlOutboundValue) throws -> BamlStreamNext<Partial> {
        if raw.wireClassFQN() == doneFQN {
            return .finished
        }
        return .value(try Partial._bamlDecode(raw))
    }
}

extension BamlStream: Equatable {
    /// Streams are live capabilities; equality is engine-resource identity.
    public static func == (lhs: BamlStream, rhs: BamlStream) -> Bool {
        lhs.handle == rhs.handle
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
        guard handle.classFQN?.isEmpty == false else {
            throw BamlDecodeError.typeMismatch(
                expected: "stream handle carrying its BAML class identity",
                got: "tagged handle without a class type"
            )
        }
        return BamlStream(handle: handle)
    }
}
