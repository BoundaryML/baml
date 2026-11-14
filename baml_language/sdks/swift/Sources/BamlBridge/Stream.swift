import Foundation

/// One `next()` poll result: a partial value, or the engine's
/// end-of-stream sentinel (`ai.stream.Done`). Distinct from
/// `Value?` because a legitimate partial can itself be null.
public enum BamlStreamNext<Value> {
    case value(Value)
    case finished
}

extension BamlStreamNext: Sendable where Value: Sendable {}
extension BamlStreamNext: Equatable where Value: Equatable {}

/// A live BAML stream (`ai.stream.Stream<Value>`), handle-backed. A partial
/// and the settled value share the one type: a partial is `Value` parsed
/// from the text received so far.
///
/// The engine holds the stream state; the wire carries an
/// `ADT_TAGGED_HEAP_HANDLE` whose table row remembers the receiver's
/// type. `next`/`final` reuse the ordinary call path with the handle as
/// the `self` receiver — the exact mechanism of Python's `_stream.py`
/// (no dedicated native stream API). A reference type on purpose:
/// consuming `next()` advances shared engine-side state, like `File`'s
/// cursor.
///
/// Each value decodes as its stream's declared item type: the stream keeps
/// the item slot it was decoded under, so a stream of values with no
/// `BamlDecodable` conformance (native tuples) decodes them too.
public final class BamlStream<Value>: @unchecked Sendable {
    private static var doneFQN: String { "ai.stream.Done" }

    public let handle: BamlHandle
    /// Exact root class identity carried by the outbound tagged handle.
    public let bamlClassFQN: String
    private let decodeValue: (BamlOutboundValue) throws -> Value

    public convenience init(handle: BamlHandle) where Value: BamlDecodable {
        self.init(handle: handle, decodeValue: Value._bamlDecode)
    }

    init(handle: BamlHandle, decodeValue: @escaping (BamlOutboundValue) throws -> Value) {
        guard let classFQN = handle.classFQN, !classFQN.isEmpty else {
            preconditionFailure("tagged stream handle is missing its carried BAML class identity")
        }
        self.handle = handle
        self.bamlClassFQN = classFQN
        self.decodeValue = decodeValue
    }

    var nextFQN: String { "\(bamlClassFQN).next" }
    var finalFQN: String { "\(bamlClassFQN).final" }

    public func nextSync() throws -> BamlStreamNext<Value> {
        try interpretNext(
            BamlRuntime.shared.callRawSync(nextFQN, args: [BamlArgument("self", handle: handle)])
        )
    }

    public func next() async throws -> BamlStreamNext<Value> {
        try interpretNext(
            await BamlRuntime.shared.callRaw(nextFQN, args: [BamlArgument("self", handle: handle)])
        )
    }

    public func finalSync() throws -> Value {
        try decodeValue(BamlRuntime.shared.callRawSync(finalFQN, args: [BamlArgument("self", handle: handle)]))
    }

    public func final() async throws -> Value {
        try decodeValue(await BamlRuntime.shared.callRaw(finalFQN, args: [BamlArgument("self", handle: handle)]))
    }

    private func interpretNext(_ raw: BamlOutboundValue) throws -> BamlStreamNext<Value> {
        if raw.wireClassFQN() == Self.doneFQN {
            return .finished
        }
        return .value(try decodeValue(raw))
    }
}

extension BamlStream: Equatable {
    /// Streams are live capabilities; equality is engine-resource identity.
    public static func == (lhs: BamlStream, rhs: BamlStream) -> Bool {
        lhs.handle == rhs.handle
    }
}

extension BamlStream: BamlLeafEncodable {
    /// A stream argument rides as its bare tagged handle (Python lifts
    /// `BamlStream` to the inner `BamlPyHandle` the same way) — never a
    /// class-value wrapper.
    public func _bamlEncode(as _: BamlSlot<BamlStream<Value>>) throws -> BamlInboundValue {
        try handle._bamlEncode(as: ._leaf(.rustType))
    }
}

extension BamlStream: BamlTyped where Value: BamlTyped {
    public static var _bamlSlot: BamlSlot<BamlStream<Value>> {
        BamlSlots.stream("ai.stream.Stream", Value._bamlSlot)
    }
}

extension BamlSlots {
    /// The slot of the stream class `fqn` of `value`s. A stream rides as its
    /// handle, so the slot states only its type, and decodes a stream whose
    /// values decode under `value`.
    public static func stream<Value>(
        _ fqn: String,
        _ value: BamlSlot<Value>
    ) -> BamlSlot<BamlStream<Value>> {
        let type = BamlTypeDescriptor.classType(fqn, typeArguments: [value.type])
        let codec = BamlSlotCodec(
            encode: { try ($0 as! BamlStream<Value>)._bamlEncode(as: ._leaf(type)) },
            decode: { try BamlStream<Value>.decode($0, decodeValue: value._bamlDecode) },
            equal: { ($0 as! BamlStream<Value>) == ($1 as! BamlStream<Value>) }
        )
        return BamlSlot(BamlSlotNode(type: type, shape: .leaf, codec: codec))
    }
}

extension BamlStream: BamlDecodable {
    /// A stream decoded with no declared slot decodes its values through
    /// their own `BamlDecodable` conformance.
    public static func _bamlDecode(_ value: BamlOutboundValue) throws -> BamlStream<Value> {
        try decode(value) { try decodeOwn($0, as: Value.self) }
    }

    static func decode(
        _ value: BamlOutboundValue,
        decodeValue: @escaping (BamlOutboundValue) throws -> Value
    ) throws -> BamlStream<Value> {
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
        return BamlStream(handle: handle, decodeValue: decodeValue)
    }
}
