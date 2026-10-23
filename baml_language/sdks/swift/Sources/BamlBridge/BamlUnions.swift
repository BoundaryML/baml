// The BAML union family: one reusable generic enum per arity, written
// once here — never generated per union shape. `int | string` is
// `BamlUnion2<Int, String>` everywhere it appears (structural identity;
// no synthesized public names). Design: sdks/swift/docs/unions-design.md
//
// `indirect` so union-typed fields break struct-recursion cycles.
// Cases are positional (`t0`, `t1`, …) in canonical BAML arm order; the
// tag is never the wire name. Decode selects the arm from wire
// metadata first, class FQN second, structural try-order last.
//
// Regenerate by editing the emitter script in git history for this
// file; arities 2–8 (bumping the cap is additive).

import Foundation

public indirect enum BamlUnion2<T0, T1> {
    case t0(T0)
    case t1(T1)

    // Type-directed construction: the arm is chosen by the argument's
    // type (insertion-stable; ambiguous at the call site when two arms
    // share a type — use the positional case then).
    public init(_ value: T0) { self = .t0(value) }
    public init(_ value: T1) { self = .t1(value) }

    /// The selected arm's payload, type-erased.
    public var anyValue: Any {
        switch self {
        case .t0(let v): return v
        case .t1(let v): return v
        }
    }

    // Per-arm accessors (nil when another arm is selected).
    public var t0: T0? { if case .t0(let v) = self { return v } else { return nil } }
    public var t1: T1? { if case .t1(let v) = self { return v } else { return nil } }

    /// Type-directed access; first matching arm wins on duplicates.
    public func value<T>(as type: T.Type) -> T? { anyValue as? T }

    /// `true` when the selected arm's payload is a `T`.
    public func holds<T>(_ type: T.Type) -> Bool { value(as: type) != nil }

    /// Exhaustive by signature: one required closure per arm.
    public func match<R>(t0 onT0: (T0) throws -> R, t1 onT1: (T1) throws -> R) rethrows -> R {
        switch self {
        case .t0(let v): return try onT0(v)
        case .t1(let v): return try onT1(v)
        }
    }

    public func match<R>(t0 onT0: (T0) async throws -> R, t1 onT1: (T1) async throws -> R) async rethrows -> R {
        switch self {
        case .t0(let v): return try await onT0(v)
        case .t1(let v): return try await onT1(v)
        }
    }
}

extension BamlUnion2: Equatable where T0: Equatable, T1: Equatable {}
extension BamlUnion2: Hashable where T0: Hashable, T1: Hashable {}
extension BamlUnion2: Sendable where T0: Sendable, T1: Sendable {}

extension BamlUnion2: BamlEncodable where T0: BamlEncodable, T1: BamlEncodable {
    // A union states nothing itself: the selected arm's value rides
    // bare, as a node of that arm's slot.
    public func _bamlEncode(as slot: BamlSlot<Self>) throws -> BamlInboundValue {
        switch self {
        case .t0(let v): return try v._bamlEncode(as: slot.unionArm(0))
        case .t1(let v): return try v._bamlEncode(as: slot.unionArm(1))
        }
    }
}

extension BamlUnion2: BamlTyped where T0: BamlTyped, T1: BamlTyped {
    public static var _bamlSlot: BamlSlot<Self> { BamlSlots.union(T0._bamlSlot, T1._bamlSlot) }
}

extension BamlSlots {
    /// The slot of a `BamlUnion2`: one slot per arm, in case order.
    public static func union<T0, T1>(_ t0: BamlSlot<T0>, _ t1: BamlSlot<T1>) -> BamlSlot<BamlUnion2<T0, T1>> {
        union([t0.node, t1.node])
    }
}

extension BamlUnion2: BamlDecodable where T0: BamlDecodable, T1: BamlDecodable {
    public static func _bamlDecode(_ v: BamlOutboundValue) throws -> Self {
        // 1. Canonical wire discriminator.
        if let selected = try v.unionSelectedType() {
            if T0._bamlDecodeSelects(selected) { return .t0(try T0._bamlDecode(v)) }
            if T1._bamlDecodeSelects(selected) { return .t1(try T1._bamlDecode(v)) }
            throw BamlDecodeError.typeMismatch(expected: "BamlUnion2", got: "selected type not present in host union")
        }
        // 2. Legacy/display metadata.
        if let arm = v.unionSelectedArm() {
            if let identity = T0._bamlArmIdentity, identity == arm { return .t0(try T0._bamlDecode(v)) }
            if let identity = T1._bamlArmIdentity, identity == arm { return .t1(try T1._bamlDecode(v)) }
        }
        // 3. Class-arm FQN off the wire class value.
        if let fqn = v.wireClassFQN() {
            if let identity = T0._bamlArmIdentity, identity == fqn { return .t0(try T0._bamlDecode(v)) }
            if let identity = T1._bamlArmIdentity, identity == fqn { return .t1(try T1._bamlDecode(v)) }
        }
        // 4. Structural fallback, declared order.
        if let value = try? T0._bamlDecode(v) { return .t0(value) }
        if let value = try? T1._bamlDecode(v) { return .t1(value) }
        throw BamlDecodeError.typeMismatch(expected: "BamlUnion2", got: "unmatched union value")
    }
}

public indirect enum BamlUnion3<T0, T1, T2> {
    case t0(T0)
    case t1(T1)
    case t2(T2)

    // Type-directed construction: the arm is chosen by the argument's
    // type (insertion-stable; ambiguous at the call site when two arms
    // share a type — use the positional case then).
    public init(_ value: T0) { self = .t0(value) }
    public init(_ value: T1) { self = .t1(value) }
    public init(_ value: T2) { self = .t2(value) }

    /// The selected arm's payload, type-erased.
    public var anyValue: Any {
        switch self {
        case .t0(let v): return v
        case .t1(let v): return v
        case .t2(let v): return v
        }
    }

    // Per-arm accessors (nil when another arm is selected).
    public var t0: T0? { if case .t0(let v) = self { return v } else { return nil } }
    public var t1: T1? { if case .t1(let v) = self { return v } else { return nil } }
    public var t2: T2? { if case .t2(let v) = self { return v } else { return nil } }

    /// Type-directed access; first matching arm wins on duplicates.
    public func value<T>(as type: T.Type) -> T? { anyValue as? T }

    /// `true` when the selected arm's payload is a `T`.
    public func holds<T>(_ type: T.Type) -> Bool { value(as: type) != nil }

    /// Exhaustive by signature: one required closure per arm.
    public func match<R>(t0 onT0: (T0) throws -> R, t1 onT1: (T1) throws -> R, t2 onT2: (T2) throws -> R) rethrows -> R {
        switch self {
        case .t0(let v): return try onT0(v)
        case .t1(let v): return try onT1(v)
        case .t2(let v): return try onT2(v)
        }
    }

    public func match<R>(t0 onT0: (T0) async throws -> R, t1 onT1: (T1) async throws -> R, t2 onT2: (T2) async throws -> R) async rethrows -> R {
        switch self {
        case .t0(let v): return try await onT0(v)
        case .t1(let v): return try await onT1(v)
        case .t2(let v): return try await onT2(v)
        }
    }
}

extension BamlUnion3: Equatable where T0: Equatable, T1: Equatable, T2: Equatable {}
extension BamlUnion3: Hashable where T0: Hashable, T1: Hashable, T2: Hashable {}
extension BamlUnion3: Sendable where T0: Sendable, T1: Sendable, T2: Sendable {}

extension BamlUnion3: BamlEncodable where T0: BamlEncodable, T1: BamlEncodable, T2: BamlEncodable {
    // A union states nothing itself: the selected arm's value rides
    // bare, as a node of that arm's slot.
    public func _bamlEncode(as slot: BamlSlot<Self>) throws -> BamlInboundValue {
        switch self {
        case .t0(let v): return try v._bamlEncode(as: slot.unionArm(0))
        case .t1(let v): return try v._bamlEncode(as: slot.unionArm(1))
        case .t2(let v): return try v._bamlEncode(as: slot.unionArm(2))
        }
    }
}

extension BamlUnion3: BamlTyped where T0: BamlTyped, T1: BamlTyped, T2: BamlTyped {
    public static var _bamlSlot: BamlSlot<Self> { BamlSlots.union(T0._bamlSlot, T1._bamlSlot, T2._bamlSlot) }
}

extension BamlSlots {
    /// The slot of a `BamlUnion3`: one slot per arm, in case order.
    public static func union<T0, T1, T2>(_ t0: BamlSlot<T0>, _ t1: BamlSlot<T1>, _ t2: BamlSlot<T2>) -> BamlSlot<BamlUnion3<T0, T1, T2>> {
        union([t0.node, t1.node, t2.node])
    }
}

extension BamlUnion3: BamlDecodable where T0: BamlDecodable, T1: BamlDecodable, T2: BamlDecodable {
    public static func _bamlDecode(_ v: BamlOutboundValue) throws -> Self {
        // 1. Canonical wire discriminator.
        if let selected = try v.unionSelectedType() {
            if T0._bamlDecodeSelects(selected) { return .t0(try T0._bamlDecode(v)) }
            if T1._bamlDecodeSelects(selected) { return .t1(try T1._bamlDecode(v)) }
            if T2._bamlDecodeSelects(selected) { return .t2(try T2._bamlDecode(v)) }
            throw BamlDecodeError.typeMismatch(expected: "BamlUnion3", got: "selected type not present in host union")
        }
        // 2. Legacy/display metadata.
        if let arm = v.unionSelectedArm() {
            if let identity = T0._bamlArmIdentity, identity == arm { return .t0(try T0._bamlDecode(v)) }
            if let identity = T1._bamlArmIdentity, identity == arm { return .t1(try T1._bamlDecode(v)) }
            if let identity = T2._bamlArmIdentity, identity == arm { return .t2(try T2._bamlDecode(v)) }
        }
        // 3. Class-arm FQN off the wire class value.
        if let fqn = v.wireClassFQN() {
            if let identity = T0._bamlArmIdentity, identity == fqn { return .t0(try T0._bamlDecode(v)) }
            if let identity = T1._bamlArmIdentity, identity == fqn { return .t1(try T1._bamlDecode(v)) }
            if let identity = T2._bamlArmIdentity, identity == fqn { return .t2(try T2._bamlDecode(v)) }
        }
        // 4. Structural fallback, declared order.
        if let value = try? T0._bamlDecode(v) { return .t0(value) }
        if let value = try? T1._bamlDecode(v) { return .t1(value) }
        if let value = try? T2._bamlDecode(v) { return .t2(value) }
        throw BamlDecodeError.typeMismatch(expected: "BamlUnion3", got: "unmatched union value")
    }
}

public indirect enum BamlUnion4<T0, T1, T2, T3> {
    case t0(T0)
    case t1(T1)
    case t2(T2)
    case t3(T3)

    // Type-directed construction: the arm is chosen by the argument's
    // type (insertion-stable; ambiguous at the call site when two arms
    // share a type — use the positional case then).
    public init(_ value: T0) { self = .t0(value) }
    public init(_ value: T1) { self = .t1(value) }
    public init(_ value: T2) { self = .t2(value) }
    public init(_ value: T3) { self = .t3(value) }

    /// The selected arm's payload, type-erased.
    public var anyValue: Any {
        switch self {
        case .t0(let v): return v
        case .t1(let v): return v
        case .t2(let v): return v
        case .t3(let v): return v
        }
    }

    // Per-arm accessors (nil when another arm is selected).
    public var t0: T0? { if case .t0(let v) = self { return v } else { return nil } }
    public var t1: T1? { if case .t1(let v) = self { return v } else { return nil } }
    public var t2: T2? { if case .t2(let v) = self { return v } else { return nil } }
    public var t3: T3? { if case .t3(let v) = self { return v } else { return nil } }

    /// Type-directed access; first matching arm wins on duplicates.
    public func value<T>(as type: T.Type) -> T? { anyValue as? T }

    /// `true` when the selected arm's payload is a `T`.
    public func holds<T>(_ type: T.Type) -> Bool { value(as: type) != nil }

    /// Exhaustive by signature: one required closure per arm.
    public func match<R>(t0 onT0: (T0) throws -> R, t1 onT1: (T1) throws -> R, t2 onT2: (T2) throws -> R, t3 onT3: (T3) throws -> R) rethrows -> R {
        switch self {
        case .t0(let v): return try onT0(v)
        case .t1(let v): return try onT1(v)
        case .t2(let v): return try onT2(v)
        case .t3(let v): return try onT3(v)
        }
    }

    public func match<R>(t0 onT0: (T0) async throws -> R, t1 onT1: (T1) async throws -> R, t2 onT2: (T2) async throws -> R, t3 onT3: (T3) async throws -> R) async rethrows -> R {
        switch self {
        case .t0(let v): return try await onT0(v)
        case .t1(let v): return try await onT1(v)
        case .t2(let v): return try await onT2(v)
        case .t3(let v): return try await onT3(v)
        }
    }
}

extension BamlUnion4: Equatable where T0: Equatable, T1: Equatable, T2: Equatable, T3: Equatable {}
extension BamlUnion4: Hashable where T0: Hashable, T1: Hashable, T2: Hashable, T3: Hashable {}
extension BamlUnion4: Sendable where T0: Sendable, T1: Sendable, T2: Sendable, T3: Sendable {}

extension BamlUnion4: BamlEncodable where T0: BamlEncodable, T1: BamlEncodable, T2: BamlEncodable, T3: BamlEncodable {
    // A union states nothing itself: the selected arm's value rides
    // bare, as a node of that arm's slot.
    public func _bamlEncode(as slot: BamlSlot<Self>) throws -> BamlInboundValue {
        switch self {
        case .t0(let v): return try v._bamlEncode(as: slot.unionArm(0))
        case .t1(let v): return try v._bamlEncode(as: slot.unionArm(1))
        case .t2(let v): return try v._bamlEncode(as: slot.unionArm(2))
        case .t3(let v): return try v._bamlEncode(as: slot.unionArm(3))
        }
    }
}

extension BamlUnion4: BamlTyped where T0: BamlTyped, T1: BamlTyped, T2: BamlTyped, T3: BamlTyped {
    public static var _bamlSlot: BamlSlot<Self> { BamlSlots.union(T0._bamlSlot, T1._bamlSlot, T2._bamlSlot, T3._bamlSlot) }
}

extension BamlSlots {
    /// The slot of a `BamlUnion4`: one slot per arm, in case order.
    public static func union<T0, T1, T2, T3>(_ t0: BamlSlot<T0>, _ t1: BamlSlot<T1>, _ t2: BamlSlot<T2>, _ t3: BamlSlot<T3>) -> BamlSlot<BamlUnion4<T0, T1, T2, T3>> {
        union([t0.node, t1.node, t2.node, t3.node])
    }
}

extension BamlUnion4: BamlDecodable where T0: BamlDecodable, T1: BamlDecodable, T2: BamlDecodable, T3: BamlDecodable {
    public static func _bamlDecode(_ v: BamlOutboundValue) throws -> Self {
        // 1. Canonical wire discriminator.
        if let selected = try v.unionSelectedType() {
            if T0._bamlDecodeSelects(selected) { return .t0(try T0._bamlDecode(v)) }
            if T1._bamlDecodeSelects(selected) { return .t1(try T1._bamlDecode(v)) }
            if T2._bamlDecodeSelects(selected) { return .t2(try T2._bamlDecode(v)) }
            if T3._bamlDecodeSelects(selected) { return .t3(try T3._bamlDecode(v)) }
            throw BamlDecodeError.typeMismatch(expected: "BamlUnion4", got: "selected type not present in host union")
        }
        // 2. Legacy/display metadata.
        if let arm = v.unionSelectedArm() {
            if let identity = T0._bamlArmIdentity, identity == arm { return .t0(try T0._bamlDecode(v)) }
            if let identity = T1._bamlArmIdentity, identity == arm { return .t1(try T1._bamlDecode(v)) }
            if let identity = T2._bamlArmIdentity, identity == arm { return .t2(try T2._bamlDecode(v)) }
            if let identity = T3._bamlArmIdentity, identity == arm { return .t3(try T3._bamlDecode(v)) }
        }
        // 3. Class-arm FQN off the wire class value.
        if let fqn = v.wireClassFQN() {
            if let identity = T0._bamlArmIdentity, identity == fqn { return .t0(try T0._bamlDecode(v)) }
            if let identity = T1._bamlArmIdentity, identity == fqn { return .t1(try T1._bamlDecode(v)) }
            if let identity = T2._bamlArmIdentity, identity == fqn { return .t2(try T2._bamlDecode(v)) }
            if let identity = T3._bamlArmIdentity, identity == fqn { return .t3(try T3._bamlDecode(v)) }
        }
        // 4. Structural fallback, declared order.
        if let value = try? T0._bamlDecode(v) { return .t0(value) }
        if let value = try? T1._bamlDecode(v) { return .t1(value) }
        if let value = try? T2._bamlDecode(v) { return .t2(value) }
        if let value = try? T3._bamlDecode(v) { return .t3(value) }
        throw BamlDecodeError.typeMismatch(expected: "BamlUnion4", got: "unmatched union value")
    }
}

public indirect enum BamlUnion5<T0, T1, T2, T3, T4> {
    case t0(T0)
    case t1(T1)
    case t2(T2)
    case t3(T3)
    case t4(T4)

    // Type-directed construction: the arm is chosen by the argument's
    // type (insertion-stable; ambiguous at the call site when two arms
    // share a type — use the positional case then).
    public init(_ value: T0) { self = .t0(value) }
    public init(_ value: T1) { self = .t1(value) }
    public init(_ value: T2) { self = .t2(value) }
    public init(_ value: T3) { self = .t3(value) }
    public init(_ value: T4) { self = .t4(value) }

    /// The selected arm's payload, type-erased.
    public var anyValue: Any {
        switch self {
        case .t0(let v): return v
        case .t1(let v): return v
        case .t2(let v): return v
        case .t3(let v): return v
        case .t4(let v): return v
        }
    }

    // Per-arm accessors (nil when another arm is selected).
    public var t0: T0? { if case .t0(let v) = self { return v } else { return nil } }
    public var t1: T1? { if case .t1(let v) = self { return v } else { return nil } }
    public var t2: T2? { if case .t2(let v) = self { return v } else { return nil } }
    public var t3: T3? { if case .t3(let v) = self { return v } else { return nil } }
    public var t4: T4? { if case .t4(let v) = self { return v } else { return nil } }

    /// Type-directed access; first matching arm wins on duplicates.
    public func value<T>(as type: T.Type) -> T? { anyValue as? T }

    /// `true` when the selected arm's payload is a `T`.
    public func holds<T>(_ type: T.Type) -> Bool { value(as: type) != nil }

    /// Exhaustive by signature: one required closure per arm.
    public func match<R>(t0 onT0: (T0) throws -> R, t1 onT1: (T1) throws -> R, t2 onT2: (T2) throws -> R, t3 onT3: (T3) throws -> R, t4 onT4: (T4) throws -> R) rethrows -> R {
        switch self {
        case .t0(let v): return try onT0(v)
        case .t1(let v): return try onT1(v)
        case .t2(let v): return try onT2(v)
        case .t3(let v): return try onT3(v)
        case .t4(let v): return try onT4(v)
        }
    }

    public func match<R>(t0 onT0: (T0) async throws -> R, t1 onT1: (T1) async throws -> R, t2 onT2: (T2) async throws -> R, t3 onT3: (T3) async throws -> R, t4 onT4: (T4) async throws -> R) async rethrows -> R {
        switch self {
        case .t0(let v): return try await onT0(v)
        case .t1(let v): return try await onT1(v)
        case .t2(let v): return try await onT2(v)
        case .t3(let v): return try await onT3(v)
        case .t4(let v): return try await onT4(v)
        }
    }
}

extension BamlUnion5: Equatable where T0: Equatable, T1: Equatable, T2: Equatable, T3: Equatable, T4: Equatable {}
extension BamlUnion5: Hashable where T0: Hashable, T1: Hashable, T2: Hashable, T3: Hashable, T4: Hashable {}
extension BamlUnion5: Sendable where T0: Sendable, T1: Sendable, T2: Sendable, T3: Sendable, T4: Sendable {}

extension BamlUnion5: BamlEncodable where T0: BamlEncodable, T1: BamlEncodable, T2: BamlEncodable, T3: BamlEncodable, T4: BamlEncodable {
    // A union states nothing itself: the selected arm's value rides
    // bare, as a node of that arm's slot.
    public func _bamlEncode(as slot: BamlSlot<Self>) throws -> BamlInboundValue {
        switch self {
        case .t0(let v): return try v._bamlEncode(as: slot.unionArm(0))
        case .t1(let v): return try v._bamlEncode(as: slot.unionArm(1))
        case .t2(let v): return try v._bamlEncode(as: slot.unionArm(2))
        case .t3(let v): return try v._bamlEncode(as: slot.unionArm(3))
        case .t4(let v): return try v._bamlEncode(as: slot.unionArm(4))
        }
    }
}

extension BamlUnion5: BamlTyped where T0: BamlTyped, T1: BamlTyped, T2: BamlTyped, T3: BamlTyped, T4: BamlTyped {
    public static var _bamlSlot: BamlSlot<Self> { BamlSlots.union(T0._bamlSlot, T1._bamlSlot, T2._bamlSlot, T3._bamlSlot, T4._bamlSlot) }
}

extension BamlSlots {
    /// The slot of a `BamlUnion5`: one slot per arm, in case order.
    public static func union<T0, T1, T2, T3, T4>(_ t0: BamlSlot<T0>, _ t1: BamlSlot<T1>, _ t2: BamlSlot<T2>, _ t3: BamlSlot<T3>, _ t4: BamlSlot<T4>) -> BamlSlot<BamlUnion5<T0, T1, T2, T3, T4>> {
        union([t0.node, t1.node, t2.node, t3.node, t4.node])
    }
}

extension BamlUnion5: BamlDecodable where T0: BamlDecodable, T1: BamlDecodable, T2: BamlDecodable, T3: BamlDecodable, T4: BamlDecodable {
    public static func _bamlDecode(_ v: BamlOutboundValue) throws -> Self {
        // 1. Canonical wire discriminator.
        if let selected = try v.unionSelectedType() {
            if T0._bamlDecodeSelects(selected) { return .t0(try T0._bamlDecode(v)) }
            if T1._bamlDecodeSelects(selected) { return .t1(try T1._bamlDecode(v)) }
            if T2._bamlDecodeSelects(selected) { return .t2(try T2._bamlDecode(v)) }
            if T3._bamlDecodeSelects(selected) { return .t3(try T3._bamlDecode(v)) }
            if T4._bamlDecodeSelects(selected) { return .t4(try T4._bamlDecode(v)) }
            throw BamlDecodeError.typeMismatch(expected: "BamlUnion5", got: "selected type not present in host union")
        }
        // 2. Legacy/display metadata.
        if let arm = v.unionSelectedArm() {
            if let identity = T0._bamlArmIdentity, identity == arm { return .t0(try T0._bamlDecode(v)) }
            if let identity = T1._bamlArmIdentity, identity == arm { return .t1(try T1._bamlDecode(v)) }
            if let identity = T2._bamlArmIdentity, identity == arm { return .t2(try T2._bamlDecode(v)) }
            if let identity = T3._bamlArmIdentity, identity == arm { return .t3(try T3._bamlDecode(v)) }
            if let identity = T4._bamlArmIdentity, identity == arm { return .t4(try T4._bamlDecode(v)) }
        }
        // 3. Class-arm FQN off the wire class value.
        if let fqn = v.wireClassFQN() {
            if let identity = T0._bamlArmIdentity, identity == fqn { return .t0(try T0._bamlDecode(v)) }
            if let identity = T1._bamlArmIdentity, identity == fqn { return .t1(try T1._bamlDecode(v)) }
            if let identity = T2._bamlArmIdentity, identity == fqn { return .t2(try T2._bamlDecode(v)) }
            if let identity = T3._bamlArmIdentity, identity == fqn { return .t3(try T3._bamlDecode(v)) }
            if let identity = T4._bamlArmIdentity, identity == fqn { return .t4(try T4._bamlDecode(v)) }
        }
        // 4. Structural fallback, declared order.
        if let value = try? T0._bamlDecode(v) { return .t0(value) }
        if let value = try? T1._bamlDecode(v) { return .t1(value) }
        if let value = try? T2._bamlDecode(v) { return .t2(value) }
        if let value = try? T3._bamlDecode(v) { return .t3(value) }
        if let value = try? T4._bamlDecode(v) { return .t4(value) }
        throw BamlDecodeError.typeMismatch(expected: "BamlUnion5", got: "unmatched union value")
    }
}

public indirect enum BamlUnion6<T0, T1, T2, T3, T4, T5> {
    case t0(T0)
    case t1(T1)
    case t2(T2)
    case t3(T3)
    case t4(T4)
    case t5(T5)

    // Type-directed construction: the arm is chosen by the argument's
    // type (insertion-stable; ambiguous at the call site when two arms
    // share a type — use the positional case then).
    public init(_ value: T0) { self = .t0(value) }
    public init(_ value: T1) { self = .t1(value) }
    public init(_ value: T2) { self = .t2(value) }
    public init(_ value: T3) { self = .t3(value) }
    public init(_ value: T4) { self = .t4(value) }
    public init(_ value: T5) { self = .t5(value) }

    /// The selected arm's payload, type-erased.
    public var anyValue: Any {
        switch self {
        case .t0(let v): return v
        case .t1(let v): return v
        case .t2(let v): return v
        case .t3(let v): return v
        case .t4(let v): return v
        case .t5(let v): return v
        }
    }

    // Per-arm accessors (nil when another arm is selected).
    public var t0: T0? { if case .t0(let v) = self { return v } else { return nil } }
    public var t1: T1? { if case .t1(let v) = self { return v } else { return nil } }
    public var t2: T2? { if case .t2(let v) = self { return v } else { return nil } }
    public var t3: T3? { if case .t3(let v) = self { return v } else { return nil } }
    public var t4: T4? { if case .t4(let v) = self { return v } else { return nil } }
    public var t5: T5? { if case .t5(let v) = self { return v } else { return nil } }

    /// Type-directed access; first matching arm wins on duplicates.
    public func value<T>(as type: T.Type) -> T? { anyValue as? T }

    /// `true` when the selected arm's payload is a `T`.
    public func holds<T>(_ type: T.Type) -> Bool { value(as: type) != nil }

    /// Exhaustive by signature: one required closure per arm.
    public func match<R>(t0 onT0: (T0) throws -> R, t1 onT1: (T1) throws -> R, t2 onT2: (T2) throws -> R, t3 onT3: (T3) throws -> R, t4 onT4: (T4) throws -> R, t5 onT5: (T5) throws -> R) rethrows -> R {
        switch self {
        case .t0(let v): return try onT0(v)
        case .t1(let v): return try onT1(v)
        case .t2(let v): return try onT2(v)
        case .t3(let v): return try onT3(v)
        case .t4(let v): return try onT4(v)
        case .t5(let v): return try onT5(v)
        }
    }

    public func match<R>(t0 onT0: (T0) async throws -> R, t1 onT1: (T1) async throws -> R, t2 onT2: (T2) async throws -> R, t3 onT3: (T3) async throws -> R, t4 onT4: (T4) async throws -> R, t5 onT5: (T5) async throws -> R) async rethrows -> R {
        switch self {
        case .t0(let v): return try await onT0(v)
        case .t1(let v): return try await onT1(v)
        case .t2(let v): return try await onT2(v)
        case .t3(let v): return try await onT3(v)
        case .t4(let v): return try await onT4(v)
        case .t5(let v): return try await onT5(v)
        }
    }
}

extension BamlUnion6: Equatable where T0: Equatable, T1: Equatable, T2: Equatable, T3: Equatable, T4: Equatable, T5: Equatable {}
extension BamlUnion6: Hashable where T0: Hashable, T1: Hashable, T2: Hashable, T3: Hashable, T4: Hashable, T5: Hashable {}
extension BamlUnion6: Sendable where T0: Sendable, T1: Sendable, T2: Sendable, T3: Sendable, T4: Sendable, T5: Sendable {}

extension BamlUnion6: BamlEncodable where T0: BamlEncodable, T1: BamlEncodable, T2: BamlEncodable, T3: BamlEncodable, T4: BamlEncodable, T5: BamlEncodable {
    // A union states nothing itself: the selected arm's value rides
    // bare, as a node of that arm's slot.
    public func _bamlEncode(as slot: BamlSlot<Self>) throws -> BamlInboundValue {
        switch self {
        case .t0(let v): return try v._bamlEncode(as: slot.unionArm(0))
        case .t1(let v): return try v._bamlEncode(as: slot.unionArm(1))
        case .t2(let v): return try v._bamlEncode(as: slot.unionArm(2))
        case .t3(let v): return try v._bamlEncode(as: slot.unionArm(3))
        case .t4(let v): return try v._bamlEncode(as: slot.unionArm(4))
        case .t5(let v): return try v._bamlEncode(as: slot.unionArm(5))
        }
    }
}

extension BamlUnion6: BamlTyped where T0: BamlTyped, T1: BamlTyped, T2: BamlTyped, T3: BamlTyped, T4: BamlTyped, T5: BamlTyped {
    public static var _bamlSlot: BamlSlot<Self> { BamlSlots.union(T0._bamlSlot, T1._bamlSlot, T2._bamlSlot, T3._bamlSlot, T4._bamlSlot, T5._bamlSlot) }
}

extension BamlSlots {
    /// The slot of a `BamlUnion6`: one slot per arm, in case order.
    public static func union<T0, T1, T2, T3, T4, T5>(_ t0: BamlSlot<T0>, _ t1: BamlSlot<T1>, _ t2: BamlSlot<T2>, _ t3: BamlSlot<T3>, _ t4: BamlSlot<T4>, _ t5: BamlSlot<T5>) -> BamlSlot<BamlUnion6<T0, T1, T2, T3, T4, T5>> {
        union([t0.node, t1.node, t2.node, t3.node, t4.node, t5.node])
    }
}

extension BamlUnion6: BamlDecodable where T0: BamlDecodable, T1: BamlDecodable, T2: BamlDecodable, T3: BamlDecodable, T4: BamlDecodable, T5: BamlDecodable {
    public static func _bamlDecode(_ v: BamlOutboundValue) throws -> Self {
        // 1. Canonical wire discriminator.
        if let selected = try v.unionSelectedType() {
            if T0._bamlDecodeSelects(selected) { return .t0(try T0._bamlDecode(v)) }
            if T1._bamlDecodeSelects(selected) { return .t1(try T1._bamlDecode(v)) }
            if T2._bamlDecodeSelects(selected) { return .t2(try T2._bamlDecode(v)) }
            if T3._bamlDecodeSelects(selected) { return .t3(try T3._bamlDecode(v)) }
            if T4._bamlDecodeSelects(selected) { return .t4(try T4._bamlDecode(v)) }
            if T5._bamlDecodeSelects(selected) { return .t5(try T5._bamlDecode(v)) }
            throw BamlDecodeError.typeMismatch(expected: "BamlUnion6", got: "selected type not present in host union")
        }
        // 2. Legacy/display metadata.
        if let arm = v.unionSelectedArm() {
            if let identity = T0._bamlArmIdentity, identity == arm { return .t0(try T0._bamlDecode(v)) }
            if let identity = T1._bamlArmIdentity, identity == arm { return .t1(try T1._bamlDecode(v)) }
            if let identity = T2._bamlArmIdentity, identity == arm { return .t2(try T2._bamlDecode(v)) }
            if let identity = T3._bamlArmIdentity, identity == arm { return .t3(try T3._bamlDecode(v)) }
            if let identity = T4._bamlArmIdentity, identity == arm { return .t4(try T4._bamlDecode(v)) }
            if let identity = T5._bamlArmIdentity, identity == arm { return .t5(try T5._bamlDecode(v)) }
        }
        // 3. Class-arm FQN off the wire class value.
        if let fqn = v.wireClassFQN() {
            if let identity = T0._bamlArmIdentity, identity == fqn { return .t0(try T0._bamlDecode(v)) }
            if let identity = T1._bamlArmIdentity, identity == fqn { return .t1(try T1._bamlDecode(v)) }
            if let identity = T2._bamlArmIdentity, identity == fqn { return .t2(try T2._bamlDecode(v)) }
            if let identity = T3._bamlArmIdentity, identity == fqn { return .t3(try T3._bamlDecode(v)) }
            if let identity = T4._bamlArmIdentity, identity == fqn { return .t4(try T4._bamlDecode(v)) }
            if let identity = T5._bamlArmIdentity, identity == fqn { return .t5(try T5._bamlDecode(v)) }
        }
        // 4. Structural fallback, declared order.
        if let value = try? T0._bamlDecode(v) { return .t0(value) }
        if let value = try? T1._bamlDecode(v) { return .t1(value) }
        if let value = try? T2._bamlDecode(v) { return .t2(value) }
        if let value = try? T3._bamlDecode(v) { return .t3(value) }
        if let value = try? T4._bamlDecode(v) { return .t4(value) }
        if let value = try? T5._bamlDecode(v) { return .t5(value) }
        throw BamlDecodeError.typeMismatch(expected: "BamlUnion6", got: "unmatched union value")
    }
}

public indirect enum BamlUnion7<T0, T1, T2, T3, T4, T5, T6> {
    case t0(T0)
    case t1(T1)
    case t2(T2)
    case t3(T3)
    case t4(T4)
    case t5(T5)
    case t6(T6)

    // Type-directed construction: the arm is chosen by the argument's
    // type (insertion-stable; ambiguous at the call site when two arms
    // share a type — use the positional case then).
    public init(_ value: T0) { self = .t0(value) }
    public init(_ value: T1) { self = .t1(value) }
    public init(_ value: T2) { self = .t2(value) }
    public init(_ value: T3) { self = .t3(value) }
    public init(_ value: T4) { self = .t4(value) }
    public init(_ value: T5) { self = .t5(value) }
    public init(_ value: T6) { self = .t6(value) }

    /// The selected arm's payload, type-erased.
    public var anyValue: Any {
        switch self {
        case .t0(let v): return v
        case .t1(let v): return v
        case .t2(let v): return v
        case .t3(let v): return v
        case .t4(let v): return v
        case .t5(let v): return v
        case .t6(let v): return v
        }
    }

    // Per-arm accessors (nil when another arm is selected).
    public var t0: T0? { if case .t0(let v) = self { return v } else { return nil } }
    public var t1: T1? { if case .t1(let v) = self { return v } else { return nil } }
    public var t2: T2? { if case .t2(let v) = self { return v } else { return nil } }
    public var t3: T3? { if case .t3(let v) = self { return v } else { return nil } }
    public var t4: T4? { if case .t4(let v) = self { return v } else { return nil } }
    public var t5: T5? { if case .t5(let v) = self { return v } else { return nil } }
    public var t6: T6? { if case .t6(let v) = self { return v } else { return nil } }

    /// Type-directed access; first matching arm wins on duplicates.
    public func value<T>(as type: T.Type) -> T? { anyValue as? T }

    /// `true` when the selected arm's payload is a `T`.
    public func holds<T>(_ type: T.Type) -> Bool { value(as: type) != nil }

    /// Exhaustive by signature: one required closure per arm.
    public func match<R>(t0 onT0: (T0) throws -> R, t1 onT1: (T1) throws -> R, t2 onT2: (T2) throws -> R, t3 onT3: (T3) throws -> R, t4 onT4: (T4) throws -> R, t5 onT5: (T5) throws -> R, t6 onT6: (T6) throws -> R) rethrows -> R {
        switch self {
        case .t0(let v): return try onT0(v)
        case .t1(let v): return try onT1(v)
        case .t2(let v): return try onT2(v)
        case .t3(let v): return try onT3(v)
        case .t4(let v): return try onT4(v)
        case .t5(let v): return try onT5(v)
        case .t6(let v): return try onT6(v)
        }
    }

    public func match<R>(t0 onT0: (T0) async throws -> R, t1 onT1: (T1) async throws -> R, t2 onT2: (T2) async throws -> R, t3 onT3: (T3) async throws -> R, t4 onT4: (T4) async throws -> R, t5 onT5: (T5) async throws -> R, t6 onT6: (T6) async throws -> R) async rethrows -> R {
        switch self {
        case .t0(let v): return try await onT0(v)
        case .t1(let v): return try await onT1(v)
        case .t2(let v): return try await onT2(v)
        case .t3(let v): return try await onT3(v)
        case .t4(let v): return try await onT4(v)
        case .t5(let v): return try await onT5(v)
        case .t6(let v): return try await onT6(v)
        }
    }
}

extension BamlUnion7: Equatable where T0: Equatable, T1: Equatable, T2: Equatable, T3: Equatable, T4: Equatable, T5: Equatable, T6: Equatable {}
extension BamlUnion7: Hashable where T0: Hashable, T1: Hashable, T2: Hashable, T3: Hashable, T4: Hashable, T5: Hashable, T6: Hashable {}
extension BamlUnion7: Sendable where T0: Sendable, T1: Sendable, T2: Sendable, T3: Sendable, T4: Sendable, T5: Sendable, T6: Sendable {}

extension BamlUnion7: BamlEncodable where T0: BamlEncodable, T1: BamlEncodable, T2: BamlEncodable, T3: BamlEncodable, T4: BamlEncodable, T5: BamlEncodable, T6: BamlEncodable {
    // A union states nothing itself: the selected arm's value rides
    // bare, as a node of that arm's slot.
    public func _bamlEncode(as slot: BamlSlot<Self>) throws -> BamlInboundValue {
        switch self {
        case .t0(let v): return try v._bamlEncode(as: slot.unionArm(0))
        case .t1(let v): return try v._bamlEncode(as: slot.unionArm(1))
        case .t2(let v): return try v._bamlEncode(as: slot.unionArm(2))
        case .t3(let v): return try v._bamlEncode(as: slot.unionArm(3))
        case .t4(let v): return try v._bamlEncode(as: slot.unionArm(4))
        case .t5(let v): return try v._bamlEncode(as: slot.unionArm(5))
        case .t6(let v): return try v._bamlEncode(as: slot.unionArm(6))
        }
    }
}

extension BamlUnion7: BamlTyped where T0: BamlTyped, T1: BamlTyped, T2: BamlTyped, T3: BamlTyped, T4: BamlTyped, T5: BamlTyped, T6: BamlTyped {
    public static var _bamlSlot: BamlSlot<Self> { BamlSlots.union(T0._bamlSlot, T1._bamlSlot, T2._bamlSlot, T3._bamlSlot, T4._bamlSlot, T5._bamlSlot, T6._bamlSlot) }
}

extension BamlSlots {
    /// The slot of a `BamlUnion7`: one slot per arm, in case order.
    public static func union<T0, T1, T2, T3, T4, T5, T6>(_ t0: BamlSlot<T0>, _ t1: BamlSlot<T1>, _ t2: BamlSlot<T2>, _ t3: BamlSlot<T3>, _ t4: BamlSlot<T4>, _ t5: BamlSlot<T5>, _ t6: BamlSlot<T6>) -> BamlSlot<BamlUnion7<T0, T1, T2, T3, T4, T5, T6>> {
        union([t0.node, t1.node, t2.node, t3.node, t4.node, t5.node, t6.node])
    }
}

extension BamlUnion7: BamlDecodable where T0: BamlDecodable, T1: BamlDecodable, T2: BamlDecodable, T3: BamlDecodable, T4: BamlDecodable, T5: BamlDecodable, T6: BamlDecodable {
    public static func _bamlDecode(_ v: BamlOutboundValue) throws -> Self {
        // 1. Canonical wire discriminator.
        if let selected = try v.unionSelectedType() {
            if T0._bamlDecodeSelects(selected) { return .t0(try T0._bamlDecode(v)) }
            if T1._bamlDecodeSelects(selected) { return .t1(try T1._bamlDecode(v)) }
            if T2._bamlDecodeSelects(selected) { return .t2(try T2._bamlDecode(v)) }
            if T3._bamlDecodeSelects(selected) { return .t3(try T3._bamlDecode(v)) }
            if T4._bamlDecodeSelects(selected) { return .t4(try T4._bamlDecode(v)) }
            if T5._bamlDecodeSelects(selected) { return .t5(try T5._bamlDecode(v)) }
            if T6._bamlDecodeSelects(selected) { return .t6(try T6._bamlDecode(v)) }
            throw BamlDecodeError.typeMismatch(expected: "BamlUnion7", got: "selected type not present in host union")
        }
        // 2. Legacy/display metadata.
        if let arm = v.unionSelectedArm() {
            if let identity = T0._bamlArmIdentity, identity == arm { return .t0(try T0._bamlDecode(v)) }
            if let identity = T1._bamlArmIdentity, identity == arm { return .t1(try T1._bamlDecode(v)) }
            if let identity = T2._bamlArmIdentity, identity == arm { return .t2(try T2._bamlDecode(v)) }
            if let identity = T3._bamlArmIdentity, identity == arm { return .t3(try T3._bamlDecode(v)) }
            if let identity = T4._bamlArmIdentity, identity == arm { return .t4(try T4._bamlDecode(v)) }
            if let identity = T5._bamlArmIdentity, identity == arm { return .t5(try T5._bamlDecode(v)) }
            if let identity = T6._bamlArmIdentity, identity == arm { return .t6(try T6._bamlDecode(v)) }
        }
        // 3. Class-arm FQN off the wire class value.
        if let fqn = v.wireClassFQN() {
            if let identity = T0._bamlArmIdentity, identity == fqn { return .t0(try T0._bamlDecode(v)) }
            if let identity = T1._bamlArmIdentity, identity == fqn { return .t1(try T1._bamlDecode(v)) }
            if let identity = T2._bamlArmIdentity, identity == fqn { return .t2(try T2._bamlDecode(v)) }
            if let identity = T3._bamlArmIdentity, identity == fqn { return .t3(try T3._bamlDecode(v)) }
            if let identity = T4._bamlArmIdentity, identity == fqn { return .t4(try T4._bamlDecode(v)) }
            if let identity = T5._bamlArmIdentity, identity == fqn { return .t5(try T5._bamlDecode(v)) }
            if let identity = T6._bamlArmIdentity, identity == fqn { return .t6(try T6._bamlDecode(v)) }
        }
        // 4. Structural fallback, declared order.
        if let value = try? T0._bamlDecode(v) { return .t0(value) }
        if let value = try? T1._bamlDecode(v) { return .t1(value) }
        if let value = try? T2._bamlDecode(v) { return .t2(value) }
        if let value = try? T3._bamlDecode(v) { return .t3(value) }
        if let value = try? T4._bamlDecode(v) { return .t4(value) }
        if let value = try? T5._bamlDecode(v) { return .t5(value) }
        if let value = try? T6._bamlDecode(v) { return .t6(value) }
        throw BamlDecodeError.typeMismatch(expected: "BamlUnion7", got: "unmatched union value")
    }
}

public indirect enum BamlUnion8<T0, T1, T2, T3, T4, T5, T6, T7> {
    case t0(T0)
    case t1(T1)
    case t2(T2)
    case t3(T3)
    case t4(T4)
    case t5(T5)
    case t6(T6)
    case t7(T7)

    // Type-directed construction: the arm is chosen by the argument's
    // type (insertion-stable; ambiguous at the call site when two arms
    // share a type — use the positional case then).
    public init(_ value: T0) { self = .t0(value) }
    public init(_ value: T1) { self = .t1(value) }
    public init(_ value: T2) { self = .t2(value) }
    public init(_ value: T3) { self = .t3(value) }
    public init(_ value: T4) { self = .t4(value) }
    public init(_ value: T5) { self = .t5(value) }
    public init(_ value: T6) { self = .t6(value) }
    public init(_ value: T7) { self = .t7(value) }

    /// The selected arm's payload, type-erased.
    public var anyValue: Any {
        switch self {
        case .t0(let v): return v
        case .t1(let v): return v
        case .t2(let v): return v
        case .t3(let v): return v
        case .t4(let v): return v
        case .t5(let v): return v
        case .t6(let v): return v
        case .t7(let v): return v
        }
    }

    // Per-arm accessors (nil when another arm is selected).
    public var t0: T0? { if case .t0(let v) = self { return v } else { return nil } }
    public var t1: T1? { if case .t1(let v) = self { return v } else { return nil } }
    public var t2: T2? { if case .t2(let v) = self { return v } else { return nil } }
    public var t3: T3? { if case .t3(let v) = self { return v } else { return nil } }
    public var t4: T4? { if case .t4(let v) = self { return v } else { return nil } }
    public var t5: T5? { if case .t5(let v) = self { return v } else { return nil } }
    public var t6: T6? { if case .t6(let v) = self { return v } else { return nil } }
    public var t7: T7? { if case .t7(let v) = self { return v } else { return nil } }

    /// Type-directed access; first matching arm wins on duplicates.
    public func value<T>(as type: T.Type) -> T? { anyValue as? T }

    /// `true` when the selected arm's payload is a `T`.
    public func holds<T>(_ type: T.Type) -> Bool { value(as: type) != nil }

    /// Exhaustive by signature: one required closure per arm.
    public func match<R>(t0 onT0: (T0) throws -> R, t1 onT1: (T1) throws -> R, t2 onT2: (T2) throws -> R, t3 onT3: (T3) throws -> R, t4 onT4: (T4) throws -> R, t5 onT5: (T5) throws -> R, t6 onT6: (T6) throws -> R, t7 onT7: (T7) throws -> R) rethrows -> R {
        switch self {
        case .t0(let v): return try onT0(v)
        case .t1(let v): return try onT1(v)
        case .t2(let v): return try onT2(v)
        case .t3(let v): return try onT3(v)
        case .t4(let v): return try onT4(v)
        case .t5(let v): return try onT5(v)
        case .t6(let v): return try onT6(v)
        case .t7(let v): return try onT7(v)
        }
    }

    public func match<R>(t0 onT0: (T0) async throws -> R, t1 onT1: (T1) async throws -> R, t2 onT2: (T2) async throws -> R, t3 onT3: (T3) async throws -> R, t4 onT4: (T4) async throws -> R, t5 onT5: (T5) async throws -> R, t6 onT6: (T6) async throws -> R, t7 onT7: (T7) async throws -> R) async rethrows -> R {
        switch self {
        case .t0(let v): return try await onT0(v)
        case .t1(let v): return try await onT1(v)
        case .t2(let v): return try await onT2(v)
        case .t3(let v): return try await onT3(v)
        case .t4(let v): return try await onT4(v)
        case .t5(let v): return try await onT5(v)
        case .t6(let v): return try await onT6(v)
        case .t7(let v): return try await onT7(v)
        }
    }
}

extension BamlUnion8: Equatable where T0: Equatable, T1: Equatable, T2: Equatable, T3: Equatable, T4: Equatable, T5: Equatable, T6: Equatable, T7: Equatable {}
extension BamlUnion8: Hashable where T0: Hashable, T1: Hashable, T2: Hashable, T3: Hashable, T4: Hashable, T5: Hashable, T6: Hashable, T7: Hashable {}
extension BamlUnion8: Sendable where T0: Sendable, T1: Sendable, T2: Sendable, T3: Sendable, T4: Sendable, T5: Sendable, T6: Sendable, T7: Sendable {}

extension BamlUnion8: BamlEncodable where T0: BamlEncodable, T1: BamlEncodable, T2: BamlEncodable, T3: BamlEncodable, T4: BamlEncodable, T5: BamlEncodable, T6: BamlEncodable, T7: BamlEncodable {
    // A union states nothing itself: the selected arm's value rides
    // bare, as a node of that arm's slot.
    public func _bamlEncode(as slot: BamlSlot<Self>) throws -> BamlInboundValue {
        switch self {
        case .t0(let v): return try v._bamlEncode(as: slot.unionArm(0))
        case .t1(let v): return try v._bamlEncode(as: slot.unionArm(1))
        case .t2(let v): return try v._bamlEncode(as: slot.unionArm(2))
        case .t3(let v): return try v._bamlEncode(as: slot.unionArm(3))
        case .t4(let v): return try v._bamlEncode(as: slot.unionArm(4))
        case .t5(let v): return try v._bamlEncode(as: slot.unionArm(5))
        case .t6(let v): return try v._bamlEncode(as: slot.unionArm(6))
        case .t7(let v): return try v._bamlEncode(as: slot.unionArm(7))
        }
    }
}

extension BamlUnion8: BamlTyped where T0: BamlTyped, T1: BamlTyped, T2: BamlTyped, T3: BamlTyped, T4: BamlTyped, T5: BamlTyped, T6: BamlTyped, T7: BamlTyped {
    public static var _bamlSlot: BamlSlot<Self> { BamlSlots.union(T0._bamlSlot, T1._bamlSlot, T2._bamlSlot, T3._bamlSlot, T4._bamlSlot, T5._bamlSlot, T6._bamlSlot, T7._bamlSlot) }
}

extension BamlSlots {
    /// The slot of a `BamlUnion8`: one slot per arm, in case order.
    public static func union<T0, T1, T2, T3, T4, T5, T6, T7>(_ t0: BamlSlot<T0>, _ t1: BamlSlot<T1>, _ t2: BamlSlot<T2>, _ t3: BamlSlot<T3>, _ t4: BamlSlot<T4>, _ t5: BamlSlot<T5>, _ t6: BamlSlot<T6>, _ t7: BamlSlot<T7>) -> BamlSlot<BamlUnion8<T0, T1, T2, T3, T4, T5, T6, T7>> {
        union([t0.node, t1.node, t2.node, t3.node, t4.node, t5.node, t6.node, t7.node])
    }
}

extension BamlUnion8: BamlDecodable where T0: BamlDecodable, T1: BamlDecodable, T2: BamlDecodable, T3: BamlDecodable, T4: BamlDecodable, T5: BamlDecodable, T6: BamlDecodable, T7: BamlDecodable {
    public static func _bamlDecode(_ v: BamlOutboundValue) throws -> Self {
        // 1. Canonical wire discriminator.
        if let selected = try v.unionSelectedType() {
            if T0._bamlDecodeSelects(selected) { return .t0(try T0._bamlDecode(v)) }
            if T1._bamlDecodeSelects(selected) { return .t1(try T1._bamlDecode(v)) }
            if T2._bamlDecodeSelects(selected) { return .t2(try T2._bamlDecode(v)) }
            if T3._bamlDecodeSelects(selected) { return .t3(try T3._bamlDecode(v)) }
            if T4._bamlDecodeSelects(selected) { return .t4(try T4._bamlDecode(v)) }
            if T5._bamlDecodeSelects(selected) { return .t5(try T5._bamlDecode(v)) }
            if T6._bamlDecodeSelects(selected) { return .t6(try T6._bamlDecode(v)) }
            if T7._bamlDecodeSelects(selected) { return .t7(try T7._bamlDecode(v)) }
            throw BamlDecodeError.typeMismatch(expected: "BamlUnion8", got: "selected type not present in host union")
        }
        // 2. Legacy/display metadata.
        if let arm = v.unionSelectedArm() {
            if let identity = T0._bamlArmIdentity, identity == arm { return .t0(try T0._bamlDecode(v)) }
            if let identity = T1._bamlArmIdentity, identity == arm { return .t1(try T1._bamlDecode(v)) }
            if let identity = T2._bamlArmIdentity, identity == arm { return .t2(try T2._bamlDecode(v)) }
            if let identity = T3._bamlArmIdentity, identity == arm { return .t3(try T3._bamlDecode(v)) }
            if let identity = T4._bamlArmIdentity, identity == arm { return .t4(try T4._bamlDecode(v)) }
            if let identity = T5._bamlArmIdentity, identity == arm { return .t5(try T5._bamlDecode(v)) }
            if let identity = T6._bamlArmIdentity, identity == arm { return .t6(try T6._bamlDecode(v)) }
            if let identity = T7._bamlArmIdentity, identity == arm { return .t7(try T7._bamlDecode(v)) }
        }
        // 3. Class-arm FQN off the wire class value.
        if let fqn = v.wireClassFQN() {
            if let identity = T0._bamlArmIdentity, identity == fqn { return .t0(try T0._bamlDecode(v)) }
            if let identity = T1._bamlArmIdentity, identity == fqn { return .t1(try T1._bamlDecode(v)) }
            if let identity = T2._bamlArmIdentity, identity == fqn { return .t2(try T2._bamlDecode(v)) }
            if let identity = T3._bamlArmIdentity, identity == fqn { return .t3(try T3._bamlDecode(v)) }
            if let identity = T4._bamlArmIdentity, identity == fqn { return .t4(try T4._bamlDecode(v)) }
            if let identity = T5._bamlArmIdentity, identity == fqn { return .t5(try T5._bamlDecode(v)) }
            if let identity = T6._bamlArmIdentity, identity == fqn { return .t6(try T6._bamlDecode(v)) }
            if let identity = T7._bamlArmIdentity, identity == fqn { return .t7(try T7._bamlDecode(v)) }
        }
        // 4. Structural fallback, declared order.
        if let value = try? T0._bamlDecode(v) { return .t0(value) }
        if let value = try? T1._bamlDecode(v) { return .t1(value) }
        if let value = try? T2._bamlDecode(v) { return .t2(value) }
        if let value = try? T3._bamlDecode(v) { return .t3(value) }
        if let value = try? T4._bamlDecode(v) { return .t4(value) }
        if let value = try? T5._bamlDecode(v) { return .t5(value) }
        if let value = try? T6._bamlDecode(v) { return .t6(value) }
        if let value = try? T7._bamlDecode(v) { return .t7(value) }
        throw BamlDecodeError.typeMismatch(expected: "BamlUnion8", got: "unmatched union value")
    }
}
