// The BAML tuple family: one reusable generic struct per arity, written
// once here -- never generated per tuple shape. `(int, string)` is
// `BamlTuple2<Int, String>` everywhere it appears: like `BamlUnionN`, the
// identity is structural, so every function naming the same tuple type
// shares one Swift type, and no generated declaration or name registration
// exists to go missing.
//
// A bare Swift tuple cannot fill a BAML position: tuples conform to no
// protocol, and every position crosses through `BamlEncodable`,
// `BamlDecodable` and `BamlTyped` (and a generated struct's fields through
// `Equatable`, `Hashable` and `Sendable`). So each position is a
// `BamlTupleN`, which carries a native tuple: `values` reads and writes it,
// `init(_:)` takes one, and `t0`, `t1`, ... read one slot each.
//
// Slots are positional and keep declaration order and repeats: unlike a
// union, nothing is sorted or merged. The slots are boxed, so a tuple
// holding the class that holds it needs no `@BamlIndirect`.
//
// A tuple wider than `BamlTuple12` crosses as a Swift array of the join of
// its slot types, exactly one element per slot (see `BamlSlots.tupleList`).
//
// Regenerate by editing the emitter script in git history for this file;
// arities 1-12 (bumping the cap is additive).

import Foundation

extension BamlSlots {
    /// The slot of a `BamlTupleN`: one slot per tuple slot, in slot order.
    static func tuple<Value>(_ items: [BamlSlotNode]) -> BamlSlot<Value> {
        BamlSlot(BamlSlotNode(type: .tuple(items.map(\.type)), shape: .tuple(items: items)))
    }

    /// The slot of a tuple too wide for the `BamlTupleN` family: a Swift
    /// array of exactly one element per slot, each of `element`, the join of
    /// the slot types. The array states the tuple of `slots`, so the engine
    /// still converts each element at its own slot; only positional typing
    /// is lost host-side, never an element.
    public static func tupleList<Element>(
        _ element: BamlSlot<Element>,
        slots: BamlTypeDescriptor...
    ) -> BamlSlot<[Element]> {
        precondition(!slots.isEmpty, "a BAML tuple has at least one slot")
        return BamlSlot(BamlSlotNode(
            type: .tuple(slots),
            shape: .tupleList(element: element.node, arity: slots.count)
        ))
    }
}

extension BamlSlot {
    /// Slot `index` of a tuple slot, as the slot of the value filling it.
    func tupleItem<Item>(_ index: Int, as _: Item.Type = Item.self) -> BamlSlot<Item> {
        guard case .tuple(let items) = node.shape, items.indices.contains(index) else {
            node.mismatch("slot \(index) of a tuple")
        }
        return BamlSlot<Item>(items[index])
    }
}

extension BamlInboundValue {
    /// A tuple node: a list stating the tuple type of `slot`, one item per
    /// slot, which the engine checks slot by slot. A slot that cannot be
    /// encoded releases the slots encoded before it.
    static func tupleNode<Value>(
        _ slot: BamlSlot<Value>,
        _ items: [() throws -> BamlInboundValue]
    ) throws -> BamlInboundValue {
        guard case .tuple = slot.node.shape else { slot.node.mismatch("a tuple") }
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
        return .listNode(encoded, stating: slot.type)
    }
}

public struct BamlTuple1<T0> {
    private final class Storage {
        let values: T0
        init(_ values: T0) { self.values = values }
    }

    private var storage: Storage

    /// The one slot. (Swift has no one-element tuple.)
    public var values: T0 {
        get { storage.values }
        set { storage = Storage(newValue) }
    }

    public init(_ t0: T0) { storage = Storage(t0) }

    // Per-slot accessors, in slot order.
    public var t0: T0 { storage.values }
}

extension BamlTuple1: Equatable where T0: Equatable {
    public static func == (lhs: Self, rhs: Self) -> Bool { lhs.t0 == rhs.t0 }
}
extension BamlTuple1: Hashable where T0: Hashable {
    public func hash(into hasher: inout Hasher) {
        hasher.combine(t0)
    }
}
// The slots are immutable once boxed: a new value replaces the box.
extension BamlTuple1: @unchecked Sendable where T0: Sendable {}

extension BamlTuple1: CustomStringConvertible {
    public var description: String { "(\(String(describing: t0)),)" }
}

extension BamlTuple1: BamlEncodable where T0: BamlEncodable {
    public func _bamlEncode(as slot: BamlSlot<Self>) throws -> BamlInboundValue {
        try BamlInboundValue.tupleNode(slot, [
            { try self.t0._bamlEncode(as: slot.tupleItem(0)) },
        ])
    }
}

extension BamlTuple1: BamlTyped where T0: BamlTyped {
    public static var _bamlSlot: BamlSlot<Self> { BamlSlots.tuple(T0._bamlSlot) }
}

extension BamlSlots {
    /// The slot of a `BamlTuple1`: one slot per tuple slot, in slot order.
    public static func tuple<T0>(_ t0: BamlSlot<T0>) -> BamlSlot<BamlTuple1<T0>> {
        tuple([t0.node])
    }
}

extension BamlTuple1: BamlDecodable where T0: BamlDecodable {
    public static func _bamlDecode(_ v: BamlOutboundValue) throws -> Self {
        let items = try tupleItems(v, arity: 1)
        return Self(
            try T0._bamlDecode(BamlOutboundValue(items[0]))
        )
    }

    /// Decode as a node of the declared tuple `slot`: each slot decodes
    /// under its own declared type, so a union slot picks the arm the
    /// engine selected.
    public static func _bamlDecode(_ v: BamlOutboundValue, as slot: BamlSlot<Self>) throws -> Self {
        let items = try tupleItems(v, arity: 1)
        return Self(
            try T0._bamlDecode(BamlOutboundValue(items[0]), as: slot.tupleItem(0))
        )
    }
}

public struct BamlTuple2<T0, T1> {
    private final class Storage {
        let values: (T0, T1)
        init(_ values: (T0, T1)) { self.values = values }
    }

    private var storage: Storage

    /// The slots as a native Swift tuple.
    public var values: (T0, T1) {
        get { storage.values }
        set { storage = Storage(newValue) }
    }

    public init(_ t0: T0, _ t1: T1) { storage = Storage((t0, t1)) }
    public init(_ values: (T0, T1)) { storage = Storage(values) }

    // Per-slot accessors, in slot order.
    public var t0: T0 { storage.values.0 }
    public var t1: T1 { storage.values.1 }
}

extension BamlTuple2: Equatable where T0: Equatable, T1: Equatable {
    public static func == (lhs: Self, rhs: Self) -> Bool { lhs.t0 == rhs.t0 && lhs.t1 == rhs.t1 }
}
extension BamlTuple2: Hashable where T0: Hashable, T1: Hashable {
    public func hash(into hasher: inout Hasher) {
        hasher.combine(t0)
        hasher.combine(t1)
    }
}
// The slots are immutable once boxed: a new value replaces the box.
extension BamlTuple2: @unchecked Sendable where T0: Sendable, T1: Sendable {}

extension BamlTuple2: CustomStringConvertible {
    public var description: String { "(\(String(describing: t0)), \(String(describing: t1)))" }
}

extension BamlTuple2: BamlEncodable where T0: BamlEncodable, T1: BamlEncodable {
    public func _bamlEncode(as slot: BamlSlot<Self>) throws -> BamlInboundValue {
        try BamlInboundValue.tupleNode(slot, [
            { try self.t0._bamlEncode(as: slot.tupleItem(0)) },
            { try self.t1._bamlEncode(as: slot.tupleItem(1)) },
        ])
    }
}

extension BamlTuple2: BamlTyped where T0: BamlTyped, T1: BamlTyped {
    public static var _bamlSlot: BamlSlot<Self> { BamlSlots.tuple(T0._bamlSlot, T1._bamlSlot) }
}

extension BamlSlots {
    /// The slot of a `BamlTuple2`: one slot per tuple slot, in slot order.
    public static func tuple<T0, T1>(_ t0: BamlSlot<T0>, _ t1: BamlSlot<T1>) -> BamlSlot<BamlTuple2<T0, T1>> {
        tuple([t0.node, t1.node])
    }
}

extension BamlTuple2: BamlDecodable where T0: BamlDecodable, T1: BamlDecodable {
    public static func _bamlDecode(_ v: BamlOutboundValue) throws -> Self {
        let items = try tupleItems(v, arity: 2)
        return Self(
            try T0._bamlDecode(BamlOutboundValue(items[0])),
            try T1._bamlDecode(BamlOutboundValue(items[1]))
        )
    }

    /// Decode as a node of the declared tuple `slot`: each slot decodes
    /// under its own declared type, so a union slot picks the arm the
    /// engine selected.
    public static func _bamlDecode(_ v: BamlOutboundValue, as slot: BamlSlot<Self>) throws -> Self {
        let items = try tupleItems(v, arity: 2)
        return Self(
            try T0._bamlDecode(BamlOutboundValue(items[0]), as: slot.tupleItem(0)),
            try T1._bamlDecode(BamlOutboundValue(items[1]), as: slot.tupleItem(1))
        )
    }
}

public struct BamlTuple3<T0, T1, T2> {
    private final class Storage {
        let values: (T0, T1, T2)
        init(_ values: (T0, T1, T2)) { self.values = values }
    }

    private var storage: Storage

    /// The slots as a native Swift tuple.
    public var values: (T0, T1, T2) {
        get { storage.values }
        set { storage = Storage(newValue) }
    }

    public init(_ t0: T0, _ t1: T1, _ t2: T2) { storage = Storage((t0, t1, t2)) }
    public init(_ values: (T0, T1, T2)) { storage = Storage(values) }

    // Per-slot accessors, in slot order.
    public var t0: T0 { storage.values.0 }
    public var t1: T1 { storage.values.1 }
    public var t2: T2 { storage.values.2 }
}

extension BamlTuple3: Equatable where T0: Equatable, T1: Equatable, T2: Equatable {
    public static func == (lhs: Self, rhs: Self) -> Bool { lhs.t0 == rhs.t0 && lhs.t1 == rhs.t1 && lhs.t2 == rhs.t2 }
}
extension BamlTuple3: Hashable where T0: Hashable, T1: Hashable, T2: Hashable {
    public func hash(into hasher: inout Hasher) {
        hasher.combine(t0)
        hasher.combine(t1)
        hasher.combine(t2)
    }
}
// The slots are immutable once boxed: a new value replaces the box.
extension BamlTuple3: @unchecked Sendable where T0: Sendable, T1: Sendable, T2: Sendable {}

extension BamlTuple3: CustomStringConvertible {
    public var description: String { "(\(String(describing: t0)), \(String(describing: t1)), \(String(describing: t2)))" }
}

extension BamlTuple3: BamlEncodable where T0: BamlEncodable, T1: BamlEncodable, T2: BamlEncodable {
    public func _bamlEncode(as slot: BamlSlot<Self>) throws -> BamlInboundValue {
        try BamlInboundValue.tupleNode(slot, [
            { try self.t0._bamlEncode(as: slot.tupleItem(0)) },
            { try self.t1._bamlEncode(as: slot.tupleItem(1)) },
            { try self.t2._bamlEncode(as: slot.tupleItem(2)) },
        ])
    }
}

extension BamlTuple3: BamlTyped where T0: BamlTyped, T1: BamlTyped, T2: BamlTyped {
    public static var _bamlSlot: BamlSlot<Self> { BamlSlots.tuple(T0._bamlSlot, T1._bamlSlot, T2._bamlSlot) }
}

extension BamlSlots {
    /// The slot of a `BamlTuple3`: one slot per tuple slot, in slot order.
    public static func tuple<T0, T1, T2>(_ t0: BamlSlot<T0>, _ t1: BamlSlot<T1>, _ t2: BamlSlot<T2>) -> BamlSlot<BamlTuple3<T0, T1, T2>> {
        tuple([t0.node, t1.node, t2.node])
    }
}

extension BamlTuple3: BamlDecodable where T0: BamlDecodable, T1: BamlDecodable, T2: BamlDecodable {
    public static func _bamlDecode(_ v: BamlOutboundValue) throws -> Self {
        let items = try tupleItems(v, arity: 3)
        return Self(
            try T0._bamlDecode(BamlOutboundValue(items[0])),
            try T1._bamlDecode(BamlOutboundValue(items[1])),
            try T2._bamlDecode(BamlOutboundValue(items[2]))
        )
    }

    /// Decode as a node of the declared tuple `slot`: each slot decodes
    /// under its own declared type, so a union slot picks the arm the
    /// engine selected.
    public static func _bamlDecode(_ v: BamlOutboundValue, as slot: BamlSlot<Self>) throws -> Self {
        let items = try tupleItems(v, arity: 3)
        return Self(
            try T0._bamlDecode(BamlOutboundValue(items[0]), as: slot.tupleItem(0)),
            try T1._bamlDecode(BamlOutboundValue(items[1]), as: slot.tupleItem(1)),
            try T2._bamlDecode(BamlOutboundValue(items[2]), as: slot.tupleItem(2))
        )
    }
}

public struct BamlTuple4<T0, T1, T2, T3> {
    private final class Storage {
        let values: (T0, T1, T2, T3)
        init(_ values: (T0, T1, T2, T3)) { self.values = values }
    }

    private var storage: Storage

    /// The slots as a native Swift tuple.
    public var values: (T0, T1, T2, T3) {
        get { storage.values }
        set { storage = Storage(newValue) }
    }

    public init(_ t0: T0, _ t1: T1, _ t2: T2, _ t3: T3) { storage = Storage((t0, t1, t2, t3)) }
    public init(_ values: (T0, T1, T2, T3)) { storage = Storage(values) }

    // Per-slot accessors, in slot order.
    public var t0: T0 { storage.values.0 }
    public var t1: T1 { storage.values.1 }
    public var t2: T2 { storage.values.2 }
    public var t3: T3 { storage.values.3 }
}

extension BamlTuple4: Equatable where T0: Equatable, T1: Equatable, T2: Equatable, T3: Equatable {
    public static func == (lhs: Self, rhs: Self) -> Bool { lhs.t0 == rhs.t0 && lhs.t1 == rhs.t1 && lhs.t2 == rhs.t2 && lhs.t3 == rhs.t3 }
}
extension BamlTuple4: Hashable where T0: Hashable, T1: Hashable, T2: Hashable, T3: Hashable {
    public func hash(into hasher: inout Hasher) {
        hasher.combine(t0)
        hasher.combine(t1)
        hasher.combine(t2)
        hasher.combine(t3)
    }
}
// The slots are immutable once boxed: a new value replaces the box.
extension BamlTuple4: @unchecked Sendable where T0: Sendable, T1: Sendable, T2: Sendable, T3: Sendable {}

extension BamlTuple4: CustomStringConvertible {
    public var description: String { "(\(String(describing: t0)), \(String(describing: t1)), \(String(describing: t2)), \(String(describing: t3)))" }
}

extension BamlTuple4: BamlEncodable where T0: BamlEncodable, T1: BamlEncodable, T2: BamlEncodable, T3: BamlEncodable {
    public func _bamlEncode(as slot: BamlSlot<Self>) throws -> BamlInboundValue {
        try BamlInboundValue.tupleNode(slot, [
            { try self.t0._bamlEncode(as: slot.tupleItem(0)) },
            { try self.t1._bamlEncode(as: slot.tupleItem(1)) },
            { try self.t2._bamlEncode(as: slot.tupleItem(2)) },
            { try self.t3._bamlEncode(as: slot.tupleItem(3)) },
        ])
    }
}

extension BamlTuple4: BamlTyped where T0: BamlTyped, T1: BamlTyped, T2: BamlTyped, T3: BamlTyped {
    public static var _bamlSlot: BamlSlot<Self> { BamlSlots.tuple(T0._bamlSlot, T1._bamlSlot, T2._bamlSlot, T3._bamlSlot) }
}

extension BamlSlots {
    /// The slot of a `BamlTuple4`: one slot per tuple slot, in slot order.
    public static func tuple<T0, T1, T2, T3>(_ t0: BamlSlot<T0>, _ t1: BamlSlot<T1>, _ t2: BamlSlot<T2>, _ t3: BamlSlot<T3>) -> BamlSlot<BamlTuple4<T0, T1, T2, T3>> {
        tuple([t0.node, t1.node, t2.node, t3.node])
    }
}

extension BamlTuple4: BamlDecodable where T0: BamlDecodable, T1: BamlDecodable, T2: BamlDecodable, T3: BamlDecodable {
    public static func _bamlDecode(_ v: BamlOutboundValue) throws -> Self {
        let items = try tupleItems(v, arity: 4)
        return Self(
            try T0._bamlDecode(BamlOutboundValue(items[0])),
            try T1._bamlDecode(BamlOutboundValue(items[1])),
            try T2._bamlDecode(BamlOutboundValue(items[2])),
            try T3._bamlDecode(BamlOutboundValue(items[3]))
        )
    }

    /// Decode as a node of the declared tuple `slot`: each slot decodes
    /// under its own declared type, so a union slot picks the arm the
    /// engine selected.
    public static func _bamlDecode(_ v: BamlOutboundValue, as slot: BamlSlot<Self>) throws -> Self {
        let items = try tupleItems(v, arity: 4)
        return Self(
            try T0._bamlDecode(BamlOutboundValue(items[0]), as: slot.tupleItem(0)),
            try T1._bamlDecode(BamlOutboundValue(items[1]), as: slot.tupleItem(1)),
            try T2._bamlDecode(BamlOutboundValue(items[2]), as: slot.tupleItem(2)),
            try T3._bamlDecode(BamlOutboundValue(items[3]), as: slot.tupleItem(3))
        )
    }
}

public struct BamlTuple5<T0, T1, T2, T3, T4> {
    private final class Storage {
        let values: (T0, T1, T2, T3, T4)
        init(_ values: (T0, T1, T2, T3, T4)) { self.values = values }
    }

    private var storage: Storage

    /// The slots as a native Swift tuple.
    public var values: (T0, T1, T2, T3, T4) {
        get { storage.values }
        set { storage = Storage(newValue) }
    }

    public init(_ t0: T0, _ t1: T1, _ t2: T2, _ t3: T3, _ t4: T4) { storage = Storage((t0, t1, t2, t3, t4)) }
    public init(_ values: (T0, T1, T2, T3, T4)) { storage = Storage(values) }

    // Per-slot accessors, in slot order.
    public var t0: T0 { storage.values.0 }
    public var t1: T1 { storage.values.1 }
    public var t2: T2 { storage.values.2 }
    public var t3: T3 { storage.values.3 }
    public var t4: T4 { storage.values.4 }
}

extension BamlTuple5: Equatable where T0: Equatable, T1: Equatable, T2: Equatable, T3: Equatable, T4: Equatable {
    public static func == (lhs: Self, rhs: Self) -> Bool { lhs.t0 == rhs.t0 && lhs.t1 == rhs.t1 && lhs.t2 == rhs.t2 && lhs.t3 == rhs.t3 && lhs.t4 == rhs.t4 }
}
extension BamlTuple5: Hashable where T0: Hashable, T1: Hashable, T2: Hashable, T3: Hashable, T4: Hashable {
    public func hash(into hasher: inout Hasher) {
        hasher.combine(t0)
        hasher.combine(t1)
        hasher.combine(t2)
        hasher.combine(t3)
        hasher.combine(t4)
    }
}
// The slots are immutable once boxed: a new value replaces the box.
extension BamlTuple5: @unchecked Sendable where T0: Sendable, T1: Sendable, T2: Sendable, T3: Sendable, T4: Sendable {}

extension BamlTuple5: CustomStringConvertible {
    public var description: String { "(\(String(describing: t0)), \(String(describing: t1)), \(String(describing: t2)), \(String(describing: t3)), \(String(describing: t4)))" }
}

extension BamlTuple5: BamlEncodable where T0: BamlEncodable, T1: BamlEncodable, T2: BamlEncodable, T3: BamlEncodable, T4: BamlEncodable {
    public func _bamlEncode(as slot: BamlSlot<Self>) throws -> BamlInboundValue {
        try BamlInboundValue.tupleNode(slot, [
            { try self.t0._bamlEncode(as: slot.tupleItem(0)) },
            { try self.t1._bamlEncode(as: slot.tupleItem(1)) },
            { try self.t2._bamlEncode(as: slot.tupleItem(2)) },
            { try self.t3._bamlEncode(as: slot.tupleItem(3)) },
            { try self.t4._bamlEncode(as: slot.tupleItem(4)) },
        ])
    }
}

extension BamlTuple5: BamlTyped where T0: BamlTyped, T1: BamlTyped, T2: BamlTyped, T3: BamlTyped, T4: BamlTyped {
    public static var _bamlSlot: BamlSlot<Self> { BamlSlots.tuple(T0._bamlSlot, T1._bamlSlot, T2._bamlSlot, T3._bamlSlot, T4._bamlSlot) }
}

extension BamlSlots {
    /// The slot of a `BamlTuple5`: one slot per tuple slot, in slot order.
    public static func tuple<T0, T1, T2, T3, T4>(_ t0: BamlSlot<T0>, _ t1: BamlSlot<T1>, _ t2: BamlSlot<T2>, _ t3: BamlSlot<T3>, _ t4: BamlSlot<T4>) -> BamlSlot<BamlTuple5<T0, T1, T2, T3, T4>> {
        tuple([t0.node, t1.node, t2.node, t3.node, t4.node])
    }
}

extension BamlTuple5: BamlDecodable where T0: BamlDecodable, T1: BamlDecodable, T2: BamlDecodable, T3: BamlDecodable, T4: BamlDecodable {
    public static func _bamlDecode(_ v: BamlOutboundValue) throws -> Self {
        let items = try tupleItems(v, arity: 5)
        return Self(
            try T0._bamlDecode(BamlOutboundValue(items[0])),
            try T1._bamlDecode(BamlOutboundValue(items[1])),
            try T2._bamlDecode(BamlOutboundValue(items[2])),
            try T3._bamlDecode(BamlOutboundValue(items[3])),
            try T4._bamlDecode(BamlOutboundValue(items[4]))
        )
    }

    /// Decode as a node of the declared tuple `slot`: each slot decodes
    /// under its own declared type, so a union slot picks the arm the
    /// engine selected.
    public static func _bamlDecode(_ v: BamlOutboundValue, as slot: BamlSlot<Self>) throws -> Self {
        let items = try tupleItems(v, arity: 5)
        return Self(
            try T0._bamlDecode(BamlOutboundValue(items[0]), as: slot.tupleItem(0)),
            try T1._bamlDecode(BamlOutboundValue(items[1]), as: slot.tupleItem(1)),
            try T2._bamlDecode(BamlOutboundValue(items[2]), as: slot.tupleItem(2)),
            try T3._bamlDecode(BamlOutboundValue(items[3]), as: slot.tupleItem(3)),
            try T4._bamlDecode(BamlOutboundValue(items[4]), as: slot.tupleItem(4))
        )
    }
}

public struct BamlTuple6<T0, T1, T2, T3, T4, T5> {
    private final class Storage {
        let values: (T0, T1, T2, T3, T4, T5)
        init(_ values: (T0, T1, T2, T3, T4, T5)) { self.values = values }
    }

    private var storage: Storage

    /// The slots as a native Swift tuple.
    public var values: (T0, T1, T2, T3, T4, T5) {
        get { storage.values }
        set { storage = Storage(newValue) }
    }

    public init(_ t0: T0, _ t1: T1, _ t2: T2, _ t3: T3, _ t4: T4, _ t5: T5) { storage = Storage((t0, t1, t2, t3, t4, t5)) }
    public init(_ values: (T0, T1, T2, T3, T4, T5)) { storage = Storage(values) }

    // Per-slot accessors, in slot order.
    public var t0: T0 { storage.values.0 }
    public var t1: T1 { storage.values.1 }
    public var t2: T2 { storage.values.2 }
    public var t3: T3 { storage.values.3 }
    public var t4: T4 { storage.values.4 }
    public var t5: T5 { storage.values.5 }
}

extension BamlTuple6: Equatable where T0: Equatable, T1: Equatable, T2: Equatable, T3: Equatable, T4: Equatable, T5: Equatable {
    public static func == (lhs: Self, rhs: Self) -> Bool { lhs.t0 == rhs.t0 && lhs.t1 == rhs.t1 && lhs.t2 == rhs.t2 && lhs.t3 == rhs.t3 && lhs.t4 == rhs.t4 && lhs.t5 == rhs.t5 }
}
extension BamlTuple6: Hashable where T0: Hashable, T1: Hashable, T2: Hashable, T3: Hashable, T4: Hashable, T5: Hashable {
    public func hash(into hasher: inout Hasher) {
        hasher.combine(t0)
        hasher.combine(t1)
        hasher.combine(t2)
        hasher.combine(t3)
        hasher.combine(t4)
        hasher.combine(t5)
    }
}
// The slots are immutable once boxed: a new value replaces the box.
extension BamlTuple6: @unchecked Sendable where T0: Sendable, T1: Sendable, T2: Sendable, T3: Sendable, T4: Sendable, T5: Sendable {}

extension BamlTuple6: CustomStringConvertible {
    public var description: String { "(\(String(describing: t0)), \(String(describing: t1)), \(String(describing: t2)), \(String(describing: t3)), \(String(describing: t4)), \(String(describing: t5)))" }
}

extension BamlTuple6: BamlEncodable where T0: BamlEncodable, T1: BamlEncodable, T2: BamlEncodable, T3: BamlEncodable, T4: BamlEncodable, T5: BamlEncodable {
    public func _bamlEncode(as slot: BamlSlot<Self>) throws -> BamlInboundValue {
        try BamlInboundValue.tupleNode(slot, [
            { try self.t0._bamlEncode(as: slot.tupleItem(0)) },
            { try self.t1._bamlEncode(as: slot.tupleItem(1)) },
            { try self.t2._bamlEncode(as: slot.tupleItem(2)) },
            { try self.t3._bamlEncode(as: slot.tupleItem(3)) },
            { try self.t4._bamlEncode(as: slot.tupleItem(4)) },
            { try self.t5._bamlEncode(as: slot.tupleItem(5)) },
        ])
    }
}

extension BamlTuple6: BamlTyped where T0: BamlTyped, T1: BamlTyped, T2: BamlTyped, T3: BamlTyped, T4: BamlTyped, T5: BamlTyped {
    public static var _bamlSlot: BamlSlot<Self> { BamlSlots.tuple(T0._bamlSlot, T1._bamlSlot, T2._bamlSlot, T3._bamlSlot, T4._bamlSlot, T5._bamlSlot) }
}

extension BamlSlots {
    /// The slot of a `BamlTuple6`: one slot per tuple slot, in slot order.
    public static func tuple<T0, T1, T2, T3, T4, T5>(_ t0: BamlSlot<T0>, _ t1: BamlSlot<T1>, _ t2: BamlSlot<T2>, _ t3: BamlSlot<T3>, _ t4: BamlSlot<T4>, _ t5: BamlSlot<T5>) -> BamlSlot<BamlTuple6<T0, T1, T2, T3, T4, T5>> {
        tuple([t0.node, t1.node, t2.node, t3.node, t4.node, t5.node])
    }
}

extension BamlTuple6: BamlDecodable where T0: BamlDecodable, T1: BamlDecodable, T2: BamlDecodable, T3: BamlDecodable, T4: BamlDecodable, T5: BamlDecodable {
    public static func _bamlDecode(_ v: BamlOutboundValue) throws -> Self {
        let items = try tupleItems(v, arity: 6)
        return Self(
            try T0._bamlDecode(BamlOutboundValue(items[0])),
            try T1._bamlDecode(BamlOutboundValue(items[1])),
            try T2._bamlDecode(BamlOutboundValue(items[2])),
            try T3._bamlDecode(BamlOutboundValue(items[3])),
            try T4._bamlDecode(BamlOutboundValue(items[4])),
            try T5._bamlDecode(BamlOutboundValue(items[5]))
        )
    }

    /// Decode as a node of the declared tuple `slot`: each slot decodes
    /// under its own declared type, so a union slot picks the arm the
    /// engine selected.
    public static func _bamlDecode(_ v: BamlOutboundValue, as slot: BamlSlot<Self>) throws -> Self {
        let items = try tupleItems(v, arity: 6)
        return Self(
            try T0._bamlDecode(BamlOutboundValue(items[0]), as: slot.tupleItem(0)),
            try T1._bamlDecode(BamlOutboundValue(items[1]), as: slot.tupleItem(1)),
            try T2._bamlDecode(BamlOutboundValue(items[2]), as: slot.tupleItem(2)),
            try T3._bamlDecode(BamlOutboundValue(items[3]), as: slot.tupleItem(3)),
            try T4._bamlDecode(BamlOutboundValue(items[4]), as: slot.tupleItem(4)),
            try T5._bamlDecode(BamlOutboundValue(items[5]), as: slot.tupleItem(5))
        )
    }
}

public struct BamlTuple7<T0, T1, T2, T3, T4, T5, T6> {
    private final class Storage {
        let values: (T0, T1, T2, T3, T4, T5, T6)
        init(_ values: (T0, T1, T2, T3, T4, T5, T6)) { self.values = values }
    }

    private var storage: Storage

    /// The slots as a native Swift tuple.
    public var values: (T0, T1, T2, T3, T4, T5, T6) {
        get { storage.values }
        set { storage = Storage(newValue) }
    }

    public init(_ t0: T0, _ t1: T1, _ t2: T2, _ t3: T3, _ t4: T4, _ t5: T5, _ t6: T6) { storage = Storage((t0, t1, t2, t3, t4, t5, t6)) }
    public init(_ values: (T0, T1, T2, T3, T4, T5, T6)) { storage = Storage(values) }

    // Per-slot accessors, in slot order.
    public var t0: T0 { storage.values.0 }
    public var t1: T1 { storage.values.1 }
    public var t2: T2 { storage.values.2 }
    public var t3: T3 { storage.values.3 }
    public var t4: T4 { storage.values.4 }
    public var t5: T5 { storage.values.5 }
    public var t6: T6 { storage.values.6 }
}

extension BamlTuple7: Equatable where T0: Equatable, T1: Equatable, T2: Equatable, T3: Equatable, T4: Equatable, T5: Equatable, T6: Equatable {
    public static func == (lhs: Self, rhs: Self) -> Bool { lhs.t0 == rhs.t0 && lhs.t1 == rhs.t1 && lhs.t2 == rhs.t2 && lhs.t3 == rhs.t3 && lhs.t4 == rhs.t4 && lhs.t5 == rhs.t5 && lhs.t6 == rhs.t6 }
}
extension BamlTuple7: Hashable where T0: Hashable, T1: Hashable, T2: Hashable, T3: Hashable, T4: Hashable, T5: Hashable, T6: Hashable {
    public func hash(into hasher: inout Hasher) {
        hasher.combine(t0)
        hasher.combine(t1)
        hasher.combine(t2)
        hasher.combine(t3)
        hasher.combine(t4)
        hasher.combine(t5)
        hasher.combine(t6)
    }
}
// The slots are immutable once boxed: a new value replaces the box.
extension BamlTuple7: @unchecked Sendable where T0: Sendable, T1: Sendable, T2: Sendable, T3: Sendable, T4: Sendable, T5: Sendable, T6: Sendable {}

extension BamlTuple7: CustomStringConvertible {
    public var description: String { "(\(String(describing: t0)), \(String(describing: t1)), \(String(describing: t2)), \(String(describing: t3)), \(String(describing: t4)), \(String(describing: t5)), \(String(describing: t6)))" }
}

extension BamlTuple7: BamlEncodable where T0: BamlEncodable, T1: BamlEncodable, T2: BamlEncodable, T3: BamlEncodable, T4: BamlEncodable, T5: BamlEncodable, T6: BamlEncodable {
    public func _bamlEncode(as slot: BamlSlot<Self>) throws -> BamlInboundValue {
        try BamlInboundValue.tupleNode(slot, [
            { try self.t0._bamlEncode(as: slot.tupleItem(0)) },
            { try self.t1._bamlEncode(as: slot.tupleItem(1)) },
            { try self.t2._bamlEncode(as: slot.tupleItem(2)) },
            { try self.t3._bamlEncode(as: slot.tupleItem(3)) },
            { try self.t4._bamlEncode(as: slot.tupleItem(4)) },
            { try self.t5._bamlEncode(as: slot.tupleItem(5)) },
            { try self.t6._bamlEncode(as: slot.tupleItem(6)) },
        ])
    }
}

extension BamlTuple7: BamlTyped where T0: BamlTyped, T1: BamlTyped, T2: BamlTyped, T3: BamlTyped, T4: BamlTyped, T5: BamlTyped, T6: BamlTyped {
    public static var _bamlSlot: BamlSlot<Self> { BamlSlots.tuple(T0._bamlSlot, T1._bamlSlot, T2._bamlSlot, T3._bamlSlot, T4._bamlSlot, T5._bamlSlot, T6._bamlSlot) }
}

extension BamlSlots {
    /// The slot of a `BamlTuple7`: one slot per tuple slot, in slot order.
    public static func tuple<T0, T1, T2, T3, T4, T5, T6>(_ t0: BamlSlot<T0>, _ t1: BamlSlot<T1>, _ t2: BamlSlot<T2>, _ t3: BamlSlot<T3>, _ t4: BamlSlot<T4>, _ t5: BamlSlot<T5>, _ t6: BamlSlot<T6>) -> BamlSlot<BamlTuple7<T0, T1, T2, T3, T4, T5, T6>> {
        tuple([t0.node, t1.node, t2.node, t3.node, t4.node, t5.node, t6.node])
    }
}

extension BamlTuple7: BamlDecodable where T0: BamlDecodable, T1: BamlDecodable, T2: BamlDecodable, T3: BamlDecodable, T4: BamlDecodable, T5: BamlDecodable, T6: BamlDecodable {
    public static func _bamlDecode(_ v: BamlOutboundValue) throws -> Self {
        let items = try tupleItems(v, arity: 7)
        return Self(
            try T0._bamlDecode(BamlOutboundValue(items[0])),
            try T1._bamlDecode(BamlOutboundValue(items[1])),
            try T2._bamlDecode(BamlOutboundValue(items[2])),
            try T3._bamlDecode(BamlOutboundValue(items[3])),
            try T4._bamlDecode(BamlOutboundValue(items[4])),
            try T5._bamlDecode(BamlOutboundValue(items[5])),
            try T6._bamlDecode(BamlOutboundValue(items[6]))
        )
    }

    /// Decode as a node of the declared tuple `slot`: each slot decodes
    /// under its own declared type, so a union slot picks the arm the
    /// engine selected.
    public static func _bamlDecode(_ v: BamlOutboundValue, as slot: BamlSlot<Self>) throws -> Self {
        let items = try tupleItems(v, arity: 7)
        return Self(
            try T0._bamlDecode(BamlOutboundValue(items[0]), as: slot.tupleItem(0)),
            try T1._bamlDecode(BamlOutboundValue(items[1]), as: slot.tupleItem(1)),
            try T2._bamlDecode(BamlOutboundValue(items[2]), as: slot.tupleItem(2)),
            try T3._bamlDecode(BamlOutboundValue(items[3]), as: slot.tupleItem(3)),
            try T4._bamlDecode(BamlOutboundValue(items[4]), as: slot.tupleItem(4)),
            try T5._bamlDecode(BamlOutboundValue(items[5]), as: slot.tupleItem(5)),
            try T6._bamlDecode(BamlOutboundValue(items[6]), as: slot.tupleItem(6))
        )
    }
}

public struct BamlTuple8<T0, T1, T2, T3, T4, T5, T6, T7> {
    private final class Storage {
        let values: (T0, T1, T2, T3, T4, T5, T6, T7)
        init(_ values: (T0, T1, T2, T3, T4, T5, T6, T7)) { self.values = values }
    }

    private var storage: Storage

    /// The slots as a native Swift tuple.
    public var values: (T0, T1, T2, T3, T4, T5, T6, T7) {
        get { storage.values }
        set { storage = Storage(newValue) }
    }

    public init(_ t0: T0, _ t1: T1, _ t2: T2, _ t3: T3, _ t4: T4, _ t5: T5, _ t6: T6, _ t7: T7) { storage = Storage((t0, t1, t2, t3, t4, t5, t6, t7)) }
    public init(_ values: (T0, T1, T2, T3, T4, T5, T6, T7)) { storage = Storage(values) }

    // Per-slot accessors, in slot order.
    public var t0: T0 { storage.values.0 }
    public var t1: T1 { storage.values.1 }
    public var t2: T2 { storage.values.2 }
    public var t3: T3 { storage.values.3 }
    public var t4: T4 { storage.values.4 }
    public var t5: T5 { storage.values.5 }
    public var t6: T6 { storage.values.6 }
    public var t7: T7 { storage.values.7 }
}

extension BamlTuple8: Equatable where T0: Equatable, T1: Equatable, T2: Equatable, T3: Equatable, T4: Equatable, T5: Equatable, T6: Equatable, T7: Equatable {
    public static func == (lhs: Self, rhs: Self) -> Bool { lhs.t0 == rhs.t0 && lhs.t1 == rhs.t1 && lhs.t2 == rhs.t2 && lhs.t3 == rhs.t3 && lhs.t4 == rhs.t4 && lhs.t5 == rhs.t5 && lhs.t6 == rhs.t6 && lhs.t7 == rhs.t7 }
}
extension BamlTuple8: Hashable where T0: Hashable, T1: Hashable, T2: Hashable, T3: Hashable, T4: Hashable, T5: Hashable, T6: Hashable, T7: Hashable {
    public func hash(into hasher: inout Hasher) {
        hasher.combine(t0)
        hasher.combine(t1)
        hasher.combine(t2)
        hasher.combine(t3)
        hasher.combine(t4)
        hasher.combine(t5)
        hasher.combine(t6)
        hasher.combine(t7)
    }
}
// The slots are immutable once boxed: a new value replaces the box.
extension BamlTuple8: @unchecked Sendable where T0: Sendable, T1: Sendable, T2: Sendable, T3: Sendable, T4: Sendable, T5: Sendable, T6: Sendable, T7: Sendable {}

extension BamlTuple8: CustomStringConvertible {
    public var description: String { "(\(String(describing: t0)), \(String(describing: t1)), \(String(describing: t2)), \(String(describing: t3)), \(String(describing: t4)), \(String(describing: t5)), \(String(describing: t6)), \(String(describing: t7)))" }
}

extension BamlTuple8: BamlEncodable where T0: BamlEncodable, T1: BamlEncodable, T2: BamlEncodable, T3: BamlEncodable, T4: BamlEncodable, T5: BamlEncodable, T6: BamlEncodable, T7: BamlEncodable {
    public func _bamlEncode(as slot: BamlSlot<Self>) throws -> BamlInboundValue {
        try BamlInboundValue.tupleNode(slot, [
            { try self.t0._bamlEncode(as: slot.tupleItem(0)) },
            { try self.t1._bamlEncode(as: slot.tupleItem(1)) },
            { try self.t2._bamlEncode(as: slot.tupleItem(2)) },
            { try self.t3._bamlEncode(as: slot.tupleItem(3)) },
            { try self.t4._bamlEncode(as: slot.tupleItem(4)) },
            { try self.t5._bamlEncode(as: slot.tupleItem(5)) },
            { try self.t6._bamlEncode(as: slot.tupleItem(6)) },
            { try self.t7._bamlEncode(as: slot.tupleItem(7)) },
        ])
    }
}

extension BamlTuple8: BamlTyped where T0: BamlTyped, T1: BamlTyped, T2: BamlTyped, T3: BamlTyped, T4: BamlTyped, T5: BamlTyped, T6: BamlTyped, T7: BamlTyped {
    public static var _bamlSlot: BamlSlot<Self> { BamlSlots.tuple(T0._bamlSlot, T1._bamlSlot, T2._bamlSlot, T3._bamlSlot, T4._bamlSlot, T5._bamlSlot, T6._bamlSlot, T7._bamlSlot) }
}

extension BamlSlots {
    /// The slot of a `BamlTuple8`: one slot per tuple slot, in slot order.
    public static func tuple<T0, T1, T2, T3, T4, T5, T6, T7>(_ t0: BamlSlot<T0>, _ t1: BamlSlot<T1>, _ t2: BamlSlot<T2>, _ t3: BamlSlot<T3>, _ t4: BamlSlot<T4>, _ t5: BamlSlot<T5>, _ t6: BamlSlot<T6>, _ t7: BamlSlot<T7>) -> BamlSlot<BamlTuple8<T0, T1, T2, T3, T4, T5, T6, T7>> {
        tuple([t0.node, t1.node, t2.node, t3.node, t4.node, t5.node, t6.node, t7.node])
    }
}

extension BamlTuple8: BamlDecodable where T0: BamlDecodable, T1: BamlDecodable, T2: BamlDecodable, T3: BamlDecodable, T4: BamlDecodable, T5: BamlDecodable, T6: BamlDecodable, T7: BamlDecodable {
    public static func _bamlDecode(_ v: BamlOutboundValue) throws -> Self {
        let items = try tupleItems(v, arity: 8)
        return Self(
            try T0._bamlDecode(BamlOutboundValue(items[0])),
            try T1._bamlDecode(BamlOutboundValue(items[1])),
            try T2._bamlDecode(BamlOutboundValue(items[2])),
            try T3._bamlDecode(BamlOutboundValue(items[3])),
            try T4._bamlDecode(BamlOutboundValue(items[4])),
            try T5._bamlDecode(BamlOutboundValue(items[5])),
            try T6._bamlDecode(BamlOutboundValue(items[6])),
            try T7._bamlDecode(BamlOutboundValue(items[7]))
        )
    }

    /// Decode as a node of the declared tuple `slot`: each slot decodes
    /// under its own declared type, so a union slot picks the arm the
    /// engine selected.
    public static func _bamlDecode(_ v: BamlOutboundValue, as slot: BamlSlot<Self>) throws -> Self {
        let items = try tupleItems(v, arity: 8)
        return Self(
            try T0._bamlDecode(BamlOutboundValue(items[0]), as: slot.tupleItem(0)),
            try T1._bamlDecode(BamlOutboundValue(items[1]), as: slot.tupleItem(1)),
            try T2._bamlDecode(BamlOutboundValue(items[2]), as: slot.tupleItem(2)),
            try T3._bamlDecode(BamlOutboundValue(items[3]), as: slot.tupleItem(3)),
            try T4._bamlDecode(BamlOutboundValue(items[4]), as: slot.tupleItem(4)),
            try T5._bamlDecode(BamlOutboundValue(items[5]), as: slot.tupleItem(5)),
            try T6._bamlDecode(BamlOutboundValue(items[6]), as: slot.tupleItem(6)),
            try T7._bamlDecode(BamlOutboundValue(items[7]), as: slot.tupleItem(7))
        )
    }
}

public struct BamlTuple9<T0, T1, T2, T3, T4, T5, T6, T7, T8> {
    private final class Storage {
        let values: (T0, T1, T2, T3, T4, T5, T6, T7, T8)
        init(_ values: (T0, T1, T2, T3, T4, T5, T6, T7, T8)) { self.values = values }
    }

    private var storage: Storage

    /// The slots as a native Swift tuple.
    public var values: (T0, T1, T2, T3, T4, T5, T6, T7, T8) {
        get { storage.values }
        set { storage = Storage(newValue) }
    }

    public init(_ t0: T0, _ t1: T1, _ t2: T2, _ t3: T3, _ t4: T4, _ t5: T5, _ t6: T6, _ t7: T7, _ t8: T8) { storage = Storage((t0, t1, t2, t3, t4, t5, t6, t7, t8)) }
    public init(_ values: (T0, T1, T2, T3, T4, T5, T6, T7, T8)) { storage = Storage(values) }

    // Per-slot accessors, in slot order.
    public var t0: T0 { storage.values.0 }
    public var t1: T1 { storage.values.1 }
    public var t2: T2 { storage.values.2 }
    public var t3: T3 { storage.values.3 }
    public var t4: T4 { storage.values.4 }
    public var t5: T5 { storage.values.5 }
    public var t6: T6 { storage.values.6 }
    public var t7: T7 { storage.values.7 }
    public var t8: T8 { storage.values.8 }
}

extension BamlTuple9: Equatable where T0: Equatable, T1: Equatable, T2: Equatable, T3: Equatable, T4: Equatable, T5: Equatable, T6: Equatable, T7: Equatable, T8: Equatable {
    public static func == (lhs: Self, rhs: Self) -> Bool { lhs.t0 == rhs.t0 && lhs.t1 == rhs.t1 && lhs.t2 == rhs.t2 && lhs.t3 == rhs.t3 && lhs.t4 == rhs.t4 && lhs.t5 == rhs.t5 && lhs.t6 == rhs.t6 && lhs.t7 == rhs.t7 && lhs.t8 == rhs.t8 }
}
extension BamlTuple9: Hashable where T0: Hashable, T1: Hashable, T2: Hashable, T3: Hashable, T4: Hashable, T5: Hashable, T6: Hashable, T7: Hashable, T8: Hashable {
    public func hash(into hasher: inout Hasher) {
        hasher.combine(t0)
        hasher.combine(t1)
        hasher.combine(t2)
        hasher.combine(t3)
        hasher.combine(t4)
        hasher.combine(t5)
        hasher.combine(t6)
        hasher.combine(t7)
        hasher.combine(t8)
    }
}
// The slots are immutable once boxed: a new value replaces the box.
extension BamlTuple9: @unchecked Sendable where T0: Sendable, T1: Sendable, T2: Sendable, T3: Sendable, T4: Sendable, T5: Sendable, T6: Sendable, T7: Sendable, T8: Sendable {}

extension BamlTuple9: CustomStringConvertible {
    public var description: String { "(\(String(describing: t0)), \(String(describing: t1)), \(String(describing: t2)), \(String(describing: t3)), \(String(describing: t4)), \(String(describing: t5)), \(String(describing: t6)), \(String(describing: t7)), \(String(describing: t8)))" }
}

extension BamlTuple9: BamlEncodable where T0: BamlEncodable, T1: BamlEncodable, T2: BamlEncodable, T3: BamlEncodable, T4: BamlEncodable, T5: BamlEncodable, T6: BamlEncodable, T7: BamlEncodable, T8: BamlEncodable {
    public func _bamlEncode(as slot: BamlSlot<Self>) throws -> BamlInboundValue {
        try BamlInboundValue.tupleNode(slot, [
            { try self.t0._bamlEncode(as: slot.tupleItem(0)) },
            { try self.t1._bamlEncode(as: slot.tupleItem(1)) },
            { try self.t2._bamlEncode(as: slot.tupleItem(2)) },
            { try self.t3._bamlEncode(as: slot.tupleItem(3)) },
            { try self.t4._bamlEncode(as: slot.tupleItem(4)) },
            { try self.t5._bamlEncode(as: slot.tupleItem(5)) },
            { try self.t6._bamlEncode(as: slot.tupleItem(6)) },
            { try self.t7._bamlEncode(as: slot.tupleItem(7)) },
            { try self.t8._bamlEncode(as: slot.tupleItem(8)) },
        ])
    }
}

extension BamlTuple9: BamlTyped where T0: BamlTyped, T1: BamlTyped, T2: BamlTyped, T3: BamlTyped, T4: BamlTyped, T5: BamlTyped, T6: BamlTyped, T7: BamlTyped, T8: BamlTyped {
    public static var _bamlSlot: BamlSlot<Self> { BamlSlots.tuple(T0._bamlSlot, T1._bamlSlot, T2._bamlSlot, T3._bamlSlot, T4._bamlSlot, T5._bamlSlot, T6._bamlSlot, T7._bamlSlot, T8._bamlSlot) }
}

extension BamlSlots {
    /// The slot of a `BamlTuple9`: one slot per tuple slot, in slot order.
    public static func tuple<T0, T1, T2, T3, T4, T5, T6, T7, T8>(_ t0: BamlSlot<T0>, _ t1: BamlSlot<T1>, _ t2: BamlSlot<T2>, _ t3: BamlSlot<T3>, _ t4: BamlSlot<T4>, _ t5: BamlSlot<T5>, _ t6: BamlSlot<T6>, _ t7: BamlSlot<T7>, _ t8: BamlSlot<T8>) -> BamlSlot<BamlTuple9<T0, T1, T2, T3, T4, T5, T6, T7, T8>> {
        tuple([t0.node, t1.node, t2.node, t3.node, t4.node, t5.node, t6.node, t7.node, t8.node])
    }
}

extension BamlTuple9: BamlDecodable where T0: BamlDecodable, T1: BamlDecodable, T2: BamlDecodable, T3: BamlDecodable, T4: BamlDecodable, T5: BamlDecodable, T6: BamlDecodable, T7: BamlDecodable, T8: BamlDecodable {
    public static func _bamlDecode(_ v: BamlOutboundValue) throws -> Self {
        let items = try tupleItems(v, arity: 9)
        return Self(
            try T0._bamlDecode(BamlOutboundValue(items[0])),
            try T1._bamlDecode(BamlOutboundValue(items[1])),
            try T2._bamlDecode(BamlOutboundValue(items[2])),
            try T3._bamlDecode(BamlOutboundValue(items[3])),
            try T4._bamlDecode(BamlOutboundValue(items[4])),
            try T5._bamlDecode(BamlOutboundValue(items[5])),
            try T6._bamlDecode(BamlOutboundValue(items[6])),
            try T7._bamlDecode(BamlOutboundValue(items[7])),
            try T8._bamlDecode(BamlOutboundValue(items[8]))
        )
    }

    /// Decode as a node of the declared tuple `slot`: each slot decodes
    /// under its own declared type, so a union slot picks the arm the
    /// engine selected.
    public static func _bamlDecode(_ v: BamlOutboundValue, as slot: BamlSlot<Self>) throws -> Self {
        let items = try tupleItems(v, arity: 9)
        return Self(
            try T0._bamlDecode(BamlOutboundValue(items[0]), as: slot.tupleItem(0)),
            try T1._bamlDecode(BamlOutboundValue(items[1]), as: slot.tupleItem(1)),
            try T2._bamlDecode(BamlOutboundValue(items[2]), as: slot.tupleItem(2)),
            try T3._bamlDecode(BamlOutboundValue(items[3]), as: slot.tupleItem(3)),
            try T4._bamlDecode(BamlOutboundValue(items[4]), as: slot.tupleItem(4)),
            try T5._bamlDecode(BamlOutboundValue(items[5]), as: slot.tupleItem(5)),
            try T6._bamlDecode(BamlOutboundValue(items[6]), as: slot.tupleItem(6)),
            try T7._bamlDecode(BamlOutboundValue(items[7]), as: slot.tupleItem(7)),
            try T8._bamlDecode(BamlOutboundValue(items[8]), as: slot.tupleItem(8))
        )
    }
}

public struct BamlTuple10<T0, T1, T2, T3, T4, T5, T6, T7, T8, T9> {
    private final class Storage {
        let values: (T0, T1, T2, T3, T4, T5, T6, T7, T8, T9)
        init(_ values: (T0, T1, T2, T3, T4, T5, T6, T7, T8, T9)) { self.values = values }
    }

    private var storage: Storage

    /// The slots as a native Swift tuple.
    public var values: (T0, T1, T2, T3, T4, T5, T6, T7, T8, T9) {
        get { storage.values }
        set { storage = Storage(newValue) }
    }

    public init(_ t0: T0, _ t1: T1, _ t2: T2, _ t3: T3, _ t4: T4, _ t5: T5, _ t6: T6, _ t7: T7, _ t8: T8, _ t9: T9) { storage = Storage((t0, t1, t2, t3, t4, t5, t6, t7, t8, t9)) }
    public init(_ values: (T0, T1, T2, T3, T4, T5, T6, T7, T8, T9)) { storage = Storage(values) }

    // Per-slot accessors, in slot order.
    public var t0: T0 { storage.values.0 }
    public var t1: T1 { storage.values.1 }
    public var t2: T2 { storage.values.2 }
    public var t3: T3 { storage.values.3 }
    public var t4: T4 { storage.values.4 }
    public var t5: T5 { storage.values.5 }
    public var t6: T6 { storage.values.6 }
    public var t7: T7 { storage.values.7 }
    public var t8: T8 { storage.values.8 }
    public var t9: T9 { storage.values.9 }
}

extension BamlTuple10: Equatable where T0: Equatable, T1: Equatable, T2: Equatable, T3: Equatable, T4: Equatable, T5: Equatable, T6: Equatable, T7: Equatable, T8: Equatable, T9: Equatable {
    public static func == (lhs: Self, rhs: Self) -> Bool { lhs.t0 == rhs.t0 && lhs.t1 == rhs.t1 && lhs.t2 == rhs.t2 && lhs.t3 == rhs.t3 && lhs.t4 == rhs.t4 && lhs.t5 == rhs.t5 && lhs.t6 == rhs.t6 && lhs.t7 == rhs.t7 && lhs.t8 == rhs.t8 && lhs.t9 == rhs.t9 }
}
extension BamlTuple10: Hashable where T0: Hashable, T1: Hashable, T2: Hashable, T3: Hashable, T4: Hashable, T5: Hashable, T6: Hashable, T7: Hashable, T8: Hashable, T9: Hashable {
    public func hash(into hasher: inout Hasher) {
        hasher.combine(t0)
        hasher.combine(t1)
        hasher.combine(t2)
        hasher.combine(t3)
        hasher.combine(t4)
        hasher.combine(t5)
        hasher.combine(t6)
        hasher.combine(t7)
        hasher.combine(t8)
        hasher.combine(t9)
    }
}
// The slots are immutable once boxed: a new value replaces the box.
extension BamlTuple10: @unchecked Sendable where T0: Sendable, T1: Sendable, T2: Sendable, T3: Sendable, T4: Sendable, T5: Sendable, T6: Sendable, T7: Sendable, T8: Sendable, T9: Sendable {}

extension BamlTuple10: CustomStringConvertible {
    public var description: String { "(\(String(describing: t0)), \(String(describing: t1)), \(String(describing: t2)), \(String(describing: t3)), \(String(describing: t4)), \(String(describing: t5)), \(String(describing: t6)), \(String(describing: t7)), \(String(describing: t8)), \(String(describing: t9)))" }
}

extension BamlTuple10: BamlEncodable where T0: BamlEncodable, T1: BamlEncodable, T2: BamlEncodable, T3: BamlEncodable, T4: BamlEncodable, T5: BamlEncodable, T6: BamlEncodable, T7: BamlEncodable, T8: BamlEncodable, T9: BamlEncodable {
    public func _bamlEncode(as slot: BamlSlot<Self>) throws -> BamlInboundValue {
        try BamlInboundValue.tupleNode(slot, [
            { try self.t0._bamlEncode(as: slot.tupleItem(0)) },
            { try self.t1._bamlEncode(as: slot.tupleItem(1)) },
            { try self.t2._bamlEncode(as: slot.tupleItem(2)) },
            { try self.t3._bamlEncode(as: slot.tupleItem(3)) },
            { try self.t4._bamlEncode(as: slot.tupleItem(4)) },
            { try self.t5._bamlEncode(as: slot.tupleItem(5)) },
            { try self.t6._bamlEncode(as: slot.tupleItem(6)) },
            { try self.t7._bamlEncode(as: slot.tupleItem(7)) },
            { try self.t8._bamlEncode(as: slot.tupleItem(8)) },
            { try self.t9._bamlEncode(as: slot.tupleItem(9)) },
        ])
    }
}

extension BamlTuple10: BamlTyped where T0: BamlTyped, T1: BamlTyped, T2: BamlTyped, T3: BamlTyped, T4: BamlTyped, T5: BamlTyped, T6: BamlTyped, T7: BamlTyped, T8: BamlTyped, T9: BamlTyped {
    public static var _bamlSlot: BamlSlot<Self> { BamlSlots.tuple(T0._bamlSlot, T1._bamlSlot, T2._bamlSlot, T3._bamlSlot, T4._bamlSlot, T5._bamlSlot, T6._bamlSlot, T7._bamlSlot, T8._bamlSlot, T9._bamlSlot) }
}

extension BamlSlots {
    /// The slot of a `BamlTuple10`: one slot per tuple slot, in slot order.
    public static func tuple<T0, T1, T2, T3, T4, T5, T6, T7, T8, T9>(_ t0: BamlSlot<T0>, _ t1: BamlSlot<T1>, _ t2: BamlSlot<T2>, _ t3: BamlSlot<T3>, _ t4: BamlSlot<T4>, _ t5: BamlSlot<T5>, _ t6: BamlSlot<T6>, _ t7: BamlSlot<T7>, _ t8: BamlSlot<T8>, _ t9: BamlSlot<T9>) -> BamlSlot<BamlTuple10<T0, T1, T2, T3, T4, T5, T6, T7, T8, T9>> {
        tuple([t0.node, t1.node, t2.node, t3.node, t4.node, t5.node, t6.node, t7.node, t8.node, t9.node])
    }
}

extension BamlTuple10: BamlDecodable where T0: BamlDecodable, T1: BamlDecodable, T2: BamlDecodable, T3: BamlDecodable, T4: BamlDecodable, T5: BamlDecodable, T6: BamlDecodable, T7: BamlDecodable, T8: BamlDecodable, T9: BamlDecodable {
    public static func _bamlDecode(_ v: BamlOutboundValue) throws -> Self {
        let items = try tupleItems(v, arity: 10)
        return Self(
            try T0._bamlDecode(BamlOutboundValue(items[0])),
            try T1._bamlDecode(BamlOutboundValue(items[1])),
            try T2._bamlDecode(BamlOutboundValue(items[2])),
            try T3._bamlDecode(BamlOutboundValue(items[3])),
            try T4._bamlDecode(BamlOutboundValue(items[4])),
            try T5._bamlDecode(BamlOutboundValue(items[5])),
            try T6._bamlDecode(BamlOutboundValue(items[6])),
            try T7._bamlDecode(BamlOutboundValue(items[7])),
            try T8._bamlDecode(BamlOutboundValue(items[8])),
            try T9._bamlDecode(BamlOutboundValue(items[9]))
        )
    }

    /// Decode as a node of the declared tuple `slot`: each slot decodes
    /// under its own declared type, so a union slot picks the arm the
    /// engine selected.
    public static func _bamlDecode(_ v: BamlOutboundValue, as slot: BamlSlot<Self>) throws -> Self {
        let items = try tupleItems(v, arity: 10)
        return Self(
            try T0._bamlDecode(BamlOutboundValue(items[0]), as: slot.tupleItem(0)),
            try T1._bamlDecode(BamlOutboundValue(items[1]), as: slot.tupleItem(1)),
            try T2._bamlDecode(BamlOutboundValue(items[2]), as: slot.tupleItem(2)),
            try T3._bamlDecode(BamlOutboundValue(items[3]), as: slot.tupleItem(3)),
            try T4._bamlDecode(BamlOutboundValue(items[4]), as: slot.tupleItem(4)),
            try T5._bamlDecode(BamlOutboundValue(items[5]), as: slot.tupleItem(5)),
            try T6._bamlDecode(BamlOutboundValue(items[6]), as: slot.tupleItem(6)),
            try T7._bamlDecode(BamlOutboundValue(items[7]), as: slot.tupleItem(7)),
            try T8._bamlDecode(BamlOutboundValue(items[8]), as: slot.tupleItem(8)),
            try T9._bamlDecode(BamlOutboundValue(items[9]), as: slot.tupleItem(9))
        )
    }
}

public struct BamlTuple11<T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10> {
    private final class Storage {
        let values: (T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10)
        init(_ values: (T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10)) { self.values = values }
    }

    private var storage: Storage

    /// The slots as a native Swift tuple.
    public var values: (T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10) {
        get { storage.values }
        set { storage = Storage(newValue) }
    }

    public init(_ t0: T0, _ t1: T1, _ t2: T2, _ t3: T3, _ t4: T4, _ t5: T5, _ t6: T6, _ t7: T7, _ t8: T8, _ t9: T9, _ t10: T10) { storage = Storage((t0, t1, t2, t3, t4, t5, t6, t7, t8, t9, t10)) }
    public init(_ values: (T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10)) { storage = Storage(values) }

    // Per-slot accessors, in slot order.
    public var t0: T0 { storage.values.0 }
    public var t1: T1 { storage.values.1 }
    public var t2: T2 { storage.values.2 }
    public var t3: T3 { storage.values.3 }
    public var t4: T4 { storage.values.4 }
    public var t5: T5 { storage.values.5 }
    public var t6: T6 { storage.values.6 }
    public var t7: T7 { storage.values.7 }
    public var t8: T8 { storage.values.8 }
    public var t9: T9 { storage.values.9 }
    public var t10: T10 { storage.values.10 }
}

extension BamlTuple11: Equatable where T0: Equatable, T1: Equatable, T2: Equatable, T3: Equatable, T4: Equatable, T5: Equatable, T6: Equatable, T7: Equatable, T8: Equatable, T9: Equatable, T10: Equatable {
    public static func == (lhs: Self, rhs: Self) -> Bool { lhs.t0 == rhs.t0 && lhs.t1 == rhs.t1 && lhs.t2 == rhs.t2 && lhs.t3 == rhs.t3 && lhs.t4 == rhs.t4 && lhs.t5 == rhs.t5 && lhs.t6 == rhs.t6 && lhs.t7 == rhs.t7 && lhs.t8 == rhs.t8 && lhs.t9 == rhs.t9 && lhs.t10 == rhs.t10 }
}
extension BamlTuple11: Hashable where T0: Hashable, T1: Hashable, T2: Hashable, T3: Hashable, T4: Hashable, T5: Hashable, T6: Hashable, T7: Hashable, T8: Hashable, T9: Hashable, T10: Hashable {
    public func hash(into hasher: inout Hasher) {
        hasher.combine(t0)
        hasher.combine(t1)
        hasher.combine(t2)
        hasher.combine(t3)
        hasher.combine(t4)
        hasher.combine(t5)
        hasher.combine(t6)
        hasher.combine(t7)
        hasher.combine(t8)
        hasher.combine(t9)
        hasher.combine(t10)
    }
}
// The slots are immutable once boxed: a new value replaces the box.
extension BamlTuple11: @unchecked Sendable where T0: Sendable, T1: Sendable, T2: Sendable, T3: Sendable, T4: Sendable, T5: Sendable, T6: Sendable, T7: Sendable, T8: Sendable, T9: Sendable, T10: Sendable {}

extension BamlTuple11: CustomStringConvertible {
    public var description: String { "(\(String(describing: t0)), \(String(describing: t1)), \(String(describing: t2)), \(String(describing: t3)), \(String(describing: t4)), \(String(describing: t5)), \(String(describing: t6)), \(String(describing: t7)), \(String(describing: t8)), \(String(describing: t9)), \(String(describing: t10)))" }
}

extension BamlTuple11: BamlEncodable where T0: BamlEncodable, T1: BamlEncodable, T2: BamlEncodable, T3: BamlEncodable, T4: BamlEncodable, T5: BamlEncodable, T6: BamlEncodable, T7: BamlEncodable, T8: BamlEncodable, T9: BamlEncodable, T10: BamlEncodable {
    public func _bamlEncode(as slot: BamlSlot<Self>) throws -> BamlInboundValue {
        try BamlInboundValue.tupleNode(slot, [
            { try self.t0._bamlEncode(as: slot.tupleItem(0)) },
            { try self.t1._bamlEncode(as: slot.tupleItem(1)) },
            { try self.t2._bamlEncode(as: slot.tupleItem(2)) },
            { try self.t3._bamlEncode(as: slot.tupleItem(3)) },
            { try self.t4._bamlEncode(as: slot.tupleItem(4)) },
            { try self.t5._bamlEncode(as: slot.tupleItem(5)) },
            { try self.t6._bamlEncode(as: slot.tupleItem(6)) },
            { try self.t7._bamlEncode(as: slot.tupleItem(7)) },
            { try self.t8._bamlEncode(as: slot.tupleItem(8)) },
            { try self.t9._bamlEncode(as: slot.tupleItem(9)) },
            { try self.t10._bamlEncode(as: slot.tupleItem(10)) },
        ])
    }
}

extension BamlTuple11: BamlTyped where T0: BamlTyped, T1: BamlTyped, T2: BamlTyped, T3: BamlTyped, T4: BamlTyped, T5: BamlTyped, T6: BamlTyped, T7: BamlTyped, T8: BamlTyped, T9: BamlTyped, T10: BamlTyped {
    public static var _bamlSlot: BamlSlot<Self> { BamlSlots.tuple(T0._bamlSlot, T1._bamlSlot, T2._bamlSlot, T3._bamlSlot, T4._bamlSlot, T5._bamlSlot, T6._bamlSlot, T7._bamlSlot, T8._bamlSlot, T9._bamlSlot, T10._bamlSlot) }
}

extension BamlSlots {
    /// The slot of a `BamlTuple11`: one slot per tuple slot, in slot order.
    public static func tuple<T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10>(_ t0: BamlSlot<T0>, _ t1: BamlSlot<T1>, _ t2: BamlSlot<T2>, _ t3: BamlSlot<T3>, _ t4: BamlSlot<T4>, _ t5: BamlSlot<T5>, _ t6: BamlSlot<T6>, _ t7: BamlSlot<T7>, _ t8: BamlSlot<T8>, _ t9: BamlSlot<T9>, _ t10: BamlSlot<T10>) -> BamlSlot<BamlTuple11<T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10>> {
        tuple([t0.node, t1.node, t2.node, t3.node, t4.node, t5.node, t6.node, t7.node, t8.node, t9.node, t10.node])
    }
}

extension BamlTuple11: BamlDecodable where T0: BamlDecodable, T1: BamlDecodable, T2: BamlDecodable, T3: BamlDecodable, T4: BamlDecodable, T5: BamlDecodable, T6: BamlDecodable, T7: BamlDecodable, T8: BamlDecodable, T9: BamlDecodable, T10: BamlDecodable {
    public static func _bamlDecode(_ v: BamlOutboundValue) throws -> Self {
        let items = try tupleItems(v, arity: 11)
        return Self(
            try T0._bamlDecode(BamlOutboundValue(items[0])),
            try T1._bamlDecode(BamlOutboundValue(items[1])),
            try T2._bamlDecode(BamlOutboundValue(items[2])),
            try T3._bamlDecode(BamlOutboundValue(items[3])),
            try T4._bamlDecode(BamlOutboundValue(items[4])),
            try T5._bamlDecode(BamlOutboundValue(items[5])),
            try T6._bamlDecode(BamlOutboundValue(items[6])),
            try T7._bamlDecode(BamlOutboundValue(items[7])),
            try T8._bamlDecode(BamlOutboundValue(items[8])),
            try T9._bamlDecode(BamlOutboundValue(items[9])),
            try T10._bamlDecode(BamlOutboundValue(items[10]))
        )
    }

    /// Decode as a node of the declared tuple `slot`: each slot decodes
    /// under its own declared type, so a union slot picks the arm the
    /// engine selected.
    public static func _bamlDecode(_ v: BamlOutboundValue, as slot: BamlSlot<Self>) throws -> Self {
        let items = try tupleItems(v, arity: 11)
        return Self(
            try T0._bamlDecode(BamlOutboundValue(items[0]), as: slot.tupleItem(0)),
            try T1._bamlDecode(BamlOutboundValue(items[1]), as: slot.tupleItem(1)),
            try T2._bamlDecode(BamlOutboundValue(items[2]), as: slot.tupleItem(2)),
            try T3._bamlDecode(BamlOutboundValue(items[3]), as: slot.tupleItem(3)),
            try T4._bamlDecode(BamlOutboundValue(items[4]), as: slot.tupleItem(4)),
            try T5._bamlDecode(BamlOutboundValue(items[5]), as: slot.tupleItem(5)),
            try T6._bamlDecode(BamlOutboundValue(items[6]), as: slot.tupleItem(6)),
            try T7._bamlDecode(BamlOutboundValue(items[7]), as: slot.tupleItem(7)),
            try T8._bamlDecode(BamlOutboundValue(items[8]), as: slot.tupleItem(8)),
            try T9._bamlDecode(BamlOutboundValue(items[9]), as: slot.tupleItem(9)),
            try T10._bamlDecode(BamlOutboundValue(items[10]), as: slot.tupleItem(10))
        )
    }
}

public struct BamlTuple12<T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11> {
    private final class Storage {
        let values: (T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11)
        init(_ values: (T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11)) { self.values = values }
    }

    private var storage: Storage

    /// The slots as a native Swift tuple.
    public var values: (T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11) {
        get { storage.values }
        set { storage = Storage(newValue) }
    }

    public init(_ t0: T0, _ t1: T1, _ t2: T2, _ t3: T3, _ t4: T4, _ t5: T5, _ t6: T6, _ t7: T7, _ t8: T8, _ t9: T9, _ t10: T10, _ t11: T11) { storage = Storage((t0, t1, t2, t3, t4, t5, t6, t7, t8, t9, t10, t11)) }
    public init(_ values: (T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11)) { storage = Storage(values) }

    // Per-slot accessors, in slot order.
    public var t0: T0 { storage.values.0 }
    public var t1: T1 { storage.values.1 }
    public var t2: T2 { storage.values.2 }
    public var t3: T3 { storage.values.3 }
    public var t4: T4 { storage.values.4 }
    public var t5: T5 { storage.values.5 }
    public var t6: T6 { storage.values.6 }
    public var t7: T7 { storage.values.7 }
    public var t8: T8 { storage.values.8 }
    public var t9: T9 { storage.values.9 }
    public var t10: T10 { storage.values.10 }
    public var t11: T11 { storage.values.11 }
}

extension BamlTuple12: Equatable where T0: Equatable, T1: Equatable, T2: Equatable, T3: Equatable, T4: Equatable, T5: Equatable, T6: Equatable, T7: Equatable, T8: Equatable, T9: Equatable, T10: Equatable, T11: Equatable {
    public static func == (lhs: Self, rhs: Self) -> Bool { lhs.t0 == rhs.t0 && lhs.t1 == rhs.t1 && lhs.t2 == rhs.t2 && lhs.t3 == rhs.t3 && lhs.t4 == rhs.t4 && lhs.t5 == rhs.t5 && lhs.t6 == rhs.t6 && lhs.t7 == rhs.t7 && lhs.t8 == rhs.t8 && lhs.t9 == rhs.t9 && lhs.t10 == rhs.t10 && lhs.t11 == rhs.t11 }
}
extension BamlTuple12: Hashable where T0: Hashable, T1: Hashable, T2: Hashable, T3: Hashable, T4: Hashable, T5: Hashable, T6: Hashable, T7: Hashable, T8: Hashable, T9: Hashable, T10: Hashable, T11: Hashable {
    public func hash(into hasher: inout Hasher) {
        hasher.combine(t0)
        hasher.combine(t1)
        hasher.combine(t2)
        hasher.combine(t3)
        hasher.combine(t4)
        hasher.combine(t5)
        hasher.combine(t6)
        hasher.combine(t7)
        hasher.combine(t8)
        hasher.combine(t9)
        hasher.combine(t10)
        hasher.combine(t11)
    }
}
// The slots are immutable once boxed: a new value replaces the box.
extension BamlTuple12: @unchecked Sendable where T0: Sendable, T1: Sendable, T2: Sendable, T3: Sendable, T4: Sendable, T5: Sendable, T6: Sendable, T7: Sendable, T8: Sendable, T9: Sendable, T10: Sendable, T11: Sendable {}

extension BamlTuple12: CustomStringConvertible {
    public var description: String { "(\(String(describing: t0)), \(String(describing: t1)), \(String(describing: t2)), \(String(describing: t3)), \(String(describing: t4)), \(String(describing: t5)), \(String(describing: t6)), \(String(describing: t7)), \(String(describing: t8)), \(String(describing: t9)), \(String(describing: t10)), \(String(describing: t11)))" }
}

extension BamlTuple12: BamlEncodable where T0: BamlEncodable, T1: BamlEncodable, T2: BamlEncodable, T3: BamlEncodable, T4: BamlEncodable, T5: BamlEncodable, T6: BamlEncodable, T7: BamlEncodable, T8: BamlEncodable, T9: BamlEncodable, T10: BamlEncodable, T11: BamlEncodable {
    public func _bamlEncode(as slot: BamlSlot<Self>) throws -> BamlInboundValue {
        try BamlInboundValue.tupleNode(slot, [
            { try self.t0._bamlEncode(as: slot.tupleItem(0)) },
            { try self.t1._bamlEncode(as: slot.tupleItem(1)) },
            { try self.t2._bamlEncode(as: slot.tupleItem(2)) },
            { try self.t3._bamlEncode(as: slot.tupleItem(3)) },
            { try self.t4._bamlEncode(as: slot.tupleItem(4)) },
            { try self.t5._bamlEncode(as: slot.tupleItem(5)) },
            { try self.t6._bamlEncode(as: slot.tupleItem(6)) },
            { try self.t7._bamlEncode(as: slot.tupleItem(7)) },
            { try self.t8._bamlEncode(as: slot.tupleItem(8)) },
            { try self.t9._bamlEncode(as: slot.tupleItem(9)) },
            { try self.t10._bamlEncode(as: slot.tupleItem(10)) },
            { try self.t11._bamlEncode(as: slot.tupleItem(11)) },
        ])
    }
}

extension BamlTuple12: BamlTyped where T0: BamlTyped, T1: BamlTyped, T2: BamlTyped, T3: BamlTyped, T4: BamlTyped, T5: BamlTyped, T6: BamlTyped, T7: BamlTyped, T8: BamlTyped, T9: BamlTyped, T10: BamlTyped, T11: BamlTyped {
    public static var _bamlSlot: BamlSlot<Self> { BamlSlots.tuple(T0._bamlSlot, T1._bamlSlot, T2._bamlSlot, T3._bamlSlot, T4._bamlSlot, T5._bamlSlot, T6._bamlSlot, T7._bamlSlot, T8._bamlSlot, T9._bamlSlot, T10._bamlSlot, T11._bamlSlot) }
}

extension BamlSlots {
    /// The slot of a `BamlTuple12`: one slot per tuple slot, in slot order.
    public static func tuple<T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11>(_ t0: BamlSlot<T0>, _ t1: BamlSlot<T1>, _ t2: BamlSlot<T2>, _ t3: BamlSlot<T3>, _ t4: BamlSlot<T4>, _ t5: BamlSlot<T5>, _ t6: BamlSlot<T6>, _ t7: BamlSlot<T7>, _ t8: BamlSlot<T8>, _ t9: BamlSlot<T9>, _ t10: BamlSlot<T10>, _ t11: BamlSlot<T11>) -> BamlSlot<BamlTuple12<T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11>> {
        tuple([t0.node, t1.node, t2.node, t3.node, t4.node, t5.node, t6.node, t7.node, t8.node, t9.node, t10.node, t11.node])
    }
}

extension BamlTuple12: BamlDecodable where T0: BamlDecodable, T1: BamlDecodable, T2: BamlDecodable, T3: BamlDecodable, T4: BamlDecodable, T5: BamlDecodable, T6: BamlDecodable, T7: BamlDecodable, T8: BamlDecodable, T9: BamlDecodable, T10: BamlDecodable, T11: BamlDecodable {
    public static func _bamlDecode(_ v: BamlOutboundValue) throws -> Self {
        let items = try tupleItems(v, arity: 12)
        return Self(
            try T0._bamlDecode(BamlOutboundValue(items[0])),
            try T1._bamlDecode(BamlOutboundValue(items[1])),
            try T2._bamlDecode(BamlOutboundValue(items[2])),
            try T3._bamlDecode(BamlOutboundValue(items[3])),
            try T4._bamlDecode(BamlOutboundValue(items[4])),
            try T5._bamlDecode(BamlOutboundValue(items[5])),
            try T6._bamlDecode(BamlOutboundValue(items[6])),
            try T7._bamlDecode(BamlOutboundValue(items[7])),
            try T8._bamlDecode(BamlOutboundValue(items[8])),
            try T9._bamlDecode(BamlOutboundValue(items[9])),
            try T10._bamlDecode(BamlOutboundValue(items[10])),
            try T11._bamlDecode(BamlOutboundValue(items[11]))
        )
    }

    /// Decode as a node of the declared tuple `slot`: each slot decodes
    /// under its own declared type, so a union slot picks the arm the
    /// engine selected.
    public static func _bamlDecode(_ v: BamlOutboundValue, as slot: BamlSlot<Self>) throws -> Self {
        let items = try tupleItems(v, arity: 12)
        return Self(
            try T0._bamlDecode(BamlOutboundValue(items[0]), as: slot.tupleItem(0)),
            try T1._bamlDecode(BamlOutboundValue(items[1]), as: slot.tupleItem(1)),
            try T2._bamlDecode(BamlOutboundValue(items[2]), as: slot.tupleItem(2)),
            try T3._bamlDecode(BamlOutboundValue(items[3]), as: slot.tupleItem(3)),
            try T4._bamlDecode(BamlOutboundValue(items[4]), as: slot.tupleItem(4)),
            try T5._bamlDecode(BamlOutboundValue(items[5]), as: slot.tupleItem(5)),
            try T6._bamlDecode(BamlOutboundValue(items[6]), as: slot.tupleItem(6)),
            try T7._bamlDecode(BamlOutboundValue(items[7]), as: slot.tupleItem(7)),
            try T8._bamlDecode(BamlOutboundValue(items[8]), as: slot.tupleItem(8)),
            try T9._bamlDecode(BamlOutboundValue(items[9]), as: slot.tupleItem(9)),
            try T10._bamlDecode(BamlOutboundValue(items[10]), as: slot.tupleItem(10)),
            try T11._bamlDecode(BamlOutboundValue(items[11]), as: slot.tupleItem(11))
        )
    }
}
