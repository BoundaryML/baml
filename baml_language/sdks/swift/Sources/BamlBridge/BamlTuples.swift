
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
        var nodes: [BamlSlotNode] = []
        for item in repeat each items {
            nodes.append(item.node)
        }
        let arity = nodes.count
        let type = BamlTypeDescriptor.tuple(nodes.map(\.type))
        let codec = BamlSlotCodec(
            encode: { value in
                let value = value as! (repeat each Item)
                var encoders: [() throws -> BamlInboundValue] = []
                for (slot, item) in repeat (each items, each value) {
                    encoders.append { try slot._bamlEncode(item) }
                }
                return try BamlInboundValue.tupleNode(stating: type, encoders)
            },
            decode: { value in
                let raw = try tupleItems(value, arity: arity)
                var index = 0
                func next<Slot>(_ slot: BamlSlot<Slot>) throws -> Slot {
                    defer { index += 1 }
                    return try decodeTupleSlot(raw, index) { try slot._bamlDecode($0) }
                }
                return (repeat try next(each items))
            },
            equal: { lhs, rhs in
                let lhs = lhs as! (repeat each Item), rhs = rhs as! (repeat each Item)
                for (slot, left, right) in repeat (each items, each lhs, each rhs) {
                    guard slot._bamlEqual(left, right) else { return false }
                }
                return true
            }
        )
        return BamlSlot(BamlSlotNode(type: type, shape: .tuple(items: nodes), codec: codec))
    }
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
