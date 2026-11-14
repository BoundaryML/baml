// BAML tuples in Swift. A tuple of two or more slots is a native Swift tuple
// at every arity: `(int, string)` is `(Int, String)` wherever it appears,
// slots in declaration order with repeats kept. Swift has no one-element
// tuple (`(A)` is just `A`), so a one-slot tuple is the `Tuple1<A>` struct
// below, which keeps `(int,)` distinct from `int`.
//
// A native tuple conforms to no protocol, so it cannot cross through
// `BamlEncodable` / `BamlDecodable` as every other value does. Its slot
// carries the codec instead: `BamlSlots.tuple` is a variadic generic function
// that knows each slot's Swift type, so it builds the tuple's codec out of
// its slots' own, and a list, map, optional or union holding a tuple carries
// one built out of the tuple's (see `BamlSlotCodec`). Variadic generic
// *functions* run on every deployment target the package supports; no
// variadic generic type is declared, which would need the macOS 14 / iOS 17
// runtime.
//
// Values cross by copy: an outbound tuple is a new Swift value, and an
// inbound one becomes a new BAML tuple, so writing to either never changes
// the other.

import Foundation

/// A BAML tuple of one slot, `(A,)`. Swift has no one-element tuple, so this
/// struct stands for one; every wider tuple is a native Swift tuple.
public struct Tuple1<A> {
    /// The one slot, BAML's `t.0`.
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

    /// Decode as a node of the declared tuple `slot`: the slot decodes under
    /// its own declared type, so a union slot picks the arm the engine
    /// selected.
    public static func _bamlDecode(_ v: BamlOutboundValue, as slot: BamlSlot<Self>) throws -> Self {
        try slot._bamlDecode(v)
    }
}

extension BamlSlots {
    /// The slot of a one-slot tuple, `(A,)`: a `Tuple1`.
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

    /// The slot of a tuple of two or more slots: a native Swift tuple, one
    /// slot per tuple slot, in slot order.
    public static func tuple<A, B, each Rest>(
        _ item0: BamlSlot<A>,
        _ item1: BamlSlot<B>,
        _ rest: repeat BamlSlot<each Rest>
    ) -> BamlSlot<(A, B, repeat each Rest)> {
        nativeTuple(item0, item1, repeat each rest)
    }

    /// The slot of the native tuple of `items`. Only `tuple(_:_:_:)` calls
    /// it, with at least two items: a pack of one would spell its single
    /// type, not a tuple.
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
    /// A tuple node: a list stating the tuple type `type`, one item per slot,
    /// each encoded once and in slot order, which the engine checks slot by
    /// slot. A slot that cannot be encoded releases the slots encoded before
    /// it.
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

/// Slot `index` of a tuple's `items`, decoded by `decode`. Slots decode in
/// slot order, so when this one fails the slots after it are no value's yet:
/// nothing adopted the engine handles they hold, so they are released here.
/// The slots before it own theirs and give them back when dropped, and the
/// failing slot's own decoder answers for its handles.
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
