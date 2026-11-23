
import Foundation

public struct Tuple1<A> {
    public var item0: A

    public init(_ item0: A) {
        self.item0 = item0
    }
}

extension Tuple1: Equatable where A: Equatable {}
extension Tuple1: Hashable where A: Hashable {}
extension Tuple1: Sendable where A: Sendable {}

extension Tuple1: CustomStringConvertible {
    public var description: String { "(\(String(describing: item0)),)" }
}

extension Tuple1: BamlEncodable where A: BamlEncodable {
    public func _bamlEncode(as slot: BamlSlot<Self>) throws -> BamlInboundValue {
        try slot._bamlEncode(self)
    }
}

extension Tuple1: BamlTyped where A: BamlTyped {
    public static var _bamlSlot: BamlSlot<Self> { BamlSlots.tuple(A._bamlSlot) }
}

extension Tuple1: BamlDecodable where A: BamlDecodable {
    public static func _bamlDecode(_ v: BamlOutboundValue) throws -> Self {
        let items = try tupleItems(v, arity: 1)
        return Self(try decodeTupleSlot(items, 0) { try A._bamlDecode($0) })
    }

    public static func _bamlDecode(_ v: BamlOutboundValue, as slot: BamlSlot<Self>) throws -> Self {
        try slot._bamlDecode(v)
    }
}

extension BamlSlots {
    public static func tuple<A>(_ item0: BamlSlot<A>) -> BamlSlot<Tuple1<A>> {
        let type = BamlTypeDescriptor.tuple([item0.type])
        let codec = BamlSlotCodec(
            encode: { value in
                let value = value as! Tuple1<A>
                return try BamlInboundValue.tupleNode(stating: type, [{ try item0._bamlEncode(value.item0) }])
            },
            decode: { value in
                let items = try tupleItems(value, arity: 1)
                return Tuple1(try decodeTupleSlot(items, 0) { try item0._bamlDecode($0) })
            },
            equal: { item0._bamlEqual(($0 as! Tuple1<A>).item0, ($1 as! Tuple1<A>).item0) }
        )
        return BamlSlot(BamlSlotNode(type: type, shape: .tuple(items: [item0.node]), codec: codec))
    }

    public static func tuple<A, B, each Rest>(
        _ item0: BamlSlot<A>,
        _ item1: BamlSlot<B>,
        _ rest: repeat BamlSlot<each Rest>
    ) -> BamlSlot<(A, B, repeat each Rest)> {
        nativeTuple(item0, item1, repeat each rest)
    }

    private static func nativeTuple<each Item>(
        _ items: repeat BamlSlot<each Item>
    ) -> BamlSlot<(repeat each Item)> {
        // Swift 6.1's IRGen crashes copying a value whose type names a pack
        // element (here `BamlSlot<each Item>`, captured or passed on). So
        // only the untyped node leaves the pack, the codec's closures hold
        // one erased codec per element, and only the pack-generic helpers
        // below touch the tuple's elements.
        var nodes: [BamlSlotNode] = []
        for item in repeat each items {
            nodes.append(item.node)
        }
        var slots: [BamlSlotCodec] = []
        for type in repeat (each Item).self {
            slots.append(erasedSlot(nodes[slots.count], as: type))
        }
        assert(slots.count == nodes.count, "\(slots.count) codecs for \(nodes.count) tuple elements")
        let arity = nodes.count
        let type = BamlTypeDescriptor.tuple(nodes.map(\.type))
        let codec = BamlSlotCodec(
            encode: { value in
                let parts = tupleParts(value, as: (repeat each Item).self)
                return try BamlInboundValue.tupleNode(
                    stating: type,
                    zip(slots, parts).map { slot, part in { try slot.encode(part) } }
                )
            },
            decode: { value in
                let raw = try tupleItems(value, arity: arity)
                var parts: [Any] = []
                parts.reserveCapacity(arity)
                for (index, slot) in slots.enumerated() {
                    parts.append(try decodeTupleSlot(raw, index, slot.decode))
                }
                return tupleBuild(parts, as: (repeat each Item).self)
            },
            equal: { lhs, rhs in
                let lhs = tupleParts(lhs, as: (repeat each Item).self)
                let rhs = tupleParts(rhs, as: (repeat each Item).self)
                return slots.indices.allSatisfy { slots[$0].equal(lhs[$0], rhs[$0]) }
            }
        )
        return BamlSlot(BamlSlotNode(type: type, shape: .tuple(items: nodes), codec: codec))
    }
}

/// The slot of `Value` at `node`, with its value type erased.
private func erasedSlot<Value>(_ node: BamlSlotNode, as _: Value.Type) -> BamlSlotCodec {
    let slot = BamlSlot<Value>(node)
    return BamlSlotCodec(
        encode: { try slot._bamlEncode($0 as! Value) },
        decode: { try slot._bamlDecode($0) },
        equal: { slot._bamlEqual($0 as! Value, $1 as! Value) }
    )
}

/// The elements of `value`, a native tuple of the given type, in order.
private func tupleParts<each Item>(_ value: Any, as _: (repeat each Item).Type) -> [Any] {
    let value = value as! (repeat each Item)
    var parts: [Any] = []
    for part in repeat each value {
        parts.append(part)
    }
    return parts
}

/// The native tuple whose elements are `parts`, which holds exactly one
/// value of each element type, in order.
private func tupleBuild<each Item>(_ parts: [Any], as _: (repeat each Item).Type) -> (repeat each Item) {
    var index = 0
    func next<Part>(_: Part.Type) -> Part {
        defer { index += 1 }
        return parts[index] as! Part
    }
    let tuple = (repeat next((each Item).self))
    assert(index == parts.count, "tuple of \(index) built from \(parts.count) parts")
    return tuple
}

extension BamlInboundValue {
    static func tupleNode(
        stating type: BamlTypeDescriptor,
        _ items: [() throws -> BamlInboundValue]
    ) throws -> BamlInboundValue {
        try BamlValueNesting.enter()
        defer { BamlValueNesting.leave() }
        var encoded: [BamlInboundValue] = []
        encoded.reserveCapacity(items.count)
        do {
            for item in items {
                encoded.append(try item())
            }
        } catch {
            for item in encoded { releaseInbound(item.raw) }
            throw error
        }
        return .listNode(encoded, stating: type)
    }
}

func decodeTupleSlot<Item>(
    _ items: [BamlBridge_Cffi_V1_BamlOutboundValue],
    _ index: Int,
    _ decode: (BamlOutboundValue) throws -> Item
) throws -> Item {
    do {
        return try decode(BamlOutboundValue(items[index]))
    } catch {
        for item in items[(index + 1)...] { releaseEngineHandles(item) }
        throw error
    }
}
