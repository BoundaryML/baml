import Foundation
import XCTest
import CBamlBridge

@testable import BamlBridge

/// `BamlTupleN` and the wide-tuple list fallback at the wire: what a tuple
/// states inbound, and which outbound values decode as one.
final class TupleCodecTests: XCTestCase {
    private func outbound(
        _ items: [BamlBridge_Cffi_V1_BamlOutboundValue],
        carrying carried: BamlTypeDescriptor? = nil
    ) -> BamlOutboundValue {
        var tuple = BamlBridge_Cffi_V1_BamlValueTuple()
        tuple.items = items
        if let carried { tuple.tupleType = carried.raw }
        var value = BamlBridge_Cffi_V1_BamlOutboundValue()
        value.tupleValue = tuple
        return BamlOutboundValue(value)
    }

    private func int(_ value: Int64) -> BamlBridge_Cffi_V1_BamlOutboundValue {
        var out = BamlBridge_Cffi_V1_BamlOutboundValue()
        out.intValue = value
        return out
    }

    private func string(_ value: String) -> BamlBridge_Cffi_V1_BamlOutboundValue {
        var out = BamlBridge_Cffi_V1_BamlOutboundValue()
        out.stringValue = value
        return out
    }

    /// A tuple goes inbound as a list stating the tuple type, slots in order.
    func testTupleStatesItsSlotTypesInOrder() throws {
        let encoded = try BamlTuple2(1, "a")._bamlEncode().raw
        guard case .tuple(let tuple)? = encoded.valueType.ty else {
            return XCTFail("expected a tuple value_type, got \(encoded.valueType)")
        }
        XCTAssertEqual(tuple.items.map(\.primitive.kind), [.bamlTyPrimitiveInt, .bamlTyPrimitiveString])
        guard case .listValue(let list)? = encoded.value else {
            return XCTFail("expected a list payload")
        }
        XCTAssertEqual(list.values.count, 2)
        XCTAssertEqual(list.values[0].intValue, 1)
        XCTAssertEqual(list.values[1].stringValue, "a")
    }

    /// A slot states its declared type, not one read back off the Swift
    /// value: `("a", int)` states the literal slot.
    func testTupleSlotStatesItsDeclaredType() throws {
        let slot = BamlSlots.tuple(BamlSlot<String>.literal("a"), BamlSlot<Int>.own)
        let encoded = try BamlTuple2("a", 1)._bamlEncode(as: slot).raw
        guard case .tuple(let tuple)? = encoded.valueType.ty else {
            return XCTFail("expected a tuple value_type")
        }
        XCTAssertEqual(tuple.items[0].literal.stringValue, "a")
    }

    func testTupleDecodesItsSlots() throws {
        let decoded = try BamlTuple2<Int, String>._bamlDecode(outbound([int(1), string("a")]))
        XCTAssertEqual(decoded, BamlTuple2(1, "a"))
        XCTAssertEqual(decoded.values.0, 1)
        XCTAssertEqual(decoded.values.1, "a")
    }

    func testTupleRejectsWrongArityAndWrongSlot() {
        XCTAssertThrowsError(try BamlTuple2<Int, String>._bamlDecode(outbound([int(1)])))
        XCTAssertThrowsError(
            try BamlTuple2<Int, String>._bamlDecode(outbound([int(1), string("a"), int(2)]))
        )
        XCTAssertThrowsError(try BamlTuple2<Int, String>._bamlDecode(outbound([string("a"), int(1)])))
    }

    /// A list is never a tuple, whatever its length.
    func testListIsNotATuple() {
        var list = BamlBridge_Cffi_V1_BamlValueList()
        list.items = [int(1), string("a")]
        var value = BamlBridge_Cffi_V1_BamlOutboundValue()
        value.listValue = list
        XCTAssertThrowsError(try BamlTuple2<Int, String>._bamlDecode(BamlOutboundValue(value)))
    }

    /// A tuple too wide for the family is an array of the join of its slot
    /// types: it states the tuple, keeps every element, and checks the arity.
    func testWideTupleListStatesTheTupleAndChecksArity() throws {
        let element = BamlSlots.union(BamlSlot<Int>.own, BamlSlot<String>.own)
        let slot = BamlSlots.tupleList(element, slots: .int, .string, .int)
        let values: [BamlUnion2<Int, String>] = [.t0(1), .t1("a"), .t0(2)]

        let encoded = try values._bamlEncode(as: slot).raw
        guard case .tuple(let tuple)? = encoded.valueType.ty else {
            return XCTFail("expected a tuple value_type, got \(encoded.valueType)")
        }
        XCTAssertEqual(
            tuple.items.map(\.primitive.kind),
            [.bamlTyPrimitiveInt, .bamlTyPrimitiveString, .bamlTyPrimitiveInt]
        )
        XCTAssertThrowsError(try Array(values.prefix(2))._bamlEncode(as: slot))

        let decoded = try [BamlUnion2<Int, String>]._bamlDecode(
            outbound([int(1), string("a"), int(2)]), as: slot
        )
        XCTAssertEqual(decoded, values)
        XCTAssertThrowsError(
            try [BamlUnion2<Int, String>]._bamlDecode(outbound([int(1), string("a")]), as: slot)
        )
    }

    /// The list fallback is a tuple's representation only: a BAML list does
    /// not fill it.
    func testWideTupleListRefusesAList() {
        let slot = BamlSlots.tupleList(BamlSlot<Int>.own, slots: .int, .int)
        var list = BamlBridge_Cffi_V1_BamlValueList()
        list.items = [int(1), int(2)]
        var value = BamlBridge_Cffi_V1_BamlOutboundValue()
        value.listValue = list
        XCTAssertThrowsError(try [Int]._bamlDecode(BamlOutboundValue(value), as: slot))
    }

    /// A tuple level counts toward the nesting bound like a list's.
    func testTupleNestingCountsTowardTheWireBound() throws {
        func nested(_ depth: Int) -> () throws -> BamlInboundValue {
            var node = BamlSlot<Int>.own.node
            var encode: () throws -> BamlInboundValue = { try (0 as Int)._bamlEncode(as: .own) }
            for _ in 0..<depth {
                let inner = encode
                node = BamlSlotNode(type: .tuple([node.type]), shape: .tuple(items: [node]))
                let outer = BamlSlot<BamlTuple1<Int>>(node)
                encode = { try BamlInboundValue.tupleNode(outer, [inner]) }
            }
            return encode
        }
        XCTAssertNoThrow(try nested(BamlValueNesting.maxDepth)())
        XCTAssertThrowsError(try nested(BamlValueNesting.maxDepth + 1)())
    }

    /// Restating a received tuple (a thrown value a host forwards) sends it
    /// back as a list stating the tuple type it carries, not one read off
    /// its items: `(int | string, bool)` holding `(1, true)` keeps its union
    /// slot.
    func testReceivedTupleIsRestatedWithItsCarriedType() throws {
        let carried = BamlTypeDescriptor.tuple([.union([.int, .string]), .bool])
        var flag = BamlBridge_Cffi_V1_BamlOutboundValue()
        flag.boolValue = true
        let restated = try restate(outbound([int(1), flag], carrying: carried).raw)
        XCTAssertEqual(restated.valueType, carried.raw)
        XCTAssertEqual(restated.listValue.values.count, 2)
        XCTAssertEqual(restated.listValue.values[0].intValue, 1)
        XCTAssertEqual(restated.listValue.values[1].boolValue, true)
    }

    /// A tuple without its type, or whose type does not match its items,
    /// cannot be restated: no slot is ever stated `unknown`.
    func testReceivedTupleWithoutItsTypeIsRefused() {
        XCTAssertThrowsError(try restate(outbound([int(1), string("a")]).raw))
        XCTAssertThrowsError(
            try restate(outbound([int(1), string("a")], carrying: .tuple([.int])).raw)
        )
        XCTAssertThrowsError(
            try restate(outbound([int(1)], carrying: .list(.int)).raw)
        )
    }

    /// A carried type naming a runtime declaration by its row cannot be
    /// stated back.
    func testReceivedTupleNamingALiveDeclarationIsRefused() {
        var cls = BamlBridge_Cffi_V1_BamlTyClass()
        cls.live = 7
        var item = BamlBridge_Cffi_V1_BamlTy()
        item.classTy = cls
        let carried = BamlTypeDescriptor.tuple([BamlTypeDescriptor(item)])
        XCTAssertThrowsError(try restate(outbound([int(1)], carrying: carried).raw))
    }

    /// Without a selection on the value, a union picks the tuple arm whose
    /// type the tuple carries, ahead of the structural try-order: an
    /// `(int, string?)` holding `(1, "a")` is that arm, not the earlier
    /// `(int, string)` arm it also fits.
    func testUnionPicksTheTupleArmByCarriedType() throws {
        typealias Arms = BamlUnion2<BamlTuple2<Int, String>, BamlTuple2<Int, String?>>
        let carried = BamlTypeDescriptor.tuple([.int, .optional(.string)])
        let value = outbound([int(1), string("a")], carrying: carried)
        XCTAssertEqual(try Arms._bamlDecode(value), .t1(BamlTuple2(1, "a")))
        XCTAssertEqual(try Arms._bamlDecode(value, as: Arms._bamlSlot), .t1(BamlTuple2(1, "a")))
        // Without a carried type, the structural try-order still decides.
        XCTAssertEqual(try Arms._bamlDecode(outbound([int(1), string("a")])), .t0(BamlTuple2(1, "a")))
        XCTAssertNil(outbound([int(1)]).carriedTupleType())
    }

    /// Type values nested in a tuple are released with the rest of a value.
    func testLiveKeysUnderATupleTypeAreFound() {
        var cls = BamlBridge_Cffi_V1_BamlTyClass()
        cls.live = 7
        var item = BamlBridge_Cffi_V1_BamlTy()
        item.classTy = cls
        var tuple = BamlBridge_Cffi_V1_BamlTyTuple()
        tuple.items = [item, item]
        var ty = BamlBridge_Cffi_V1_BamlTy()
        ty.tuple = tuple
        XCTAssertEqual(liveKeys(ty), [7, 7])
    }

    // MARK: - Generated tuple structs

    /// A generated tuple struct encodes as a list stating its tuple type,
    /// decodes only from a tuple of its arity, and checks each slot.
    func testGeneratedTupleStructStatesItsTypeAndChecksEachSlot() throws {
        let value = Named3(t0: 1, t1: "a", t2: 2)
        let encoded = try value._bamlEncode().raw
        XCTAssertEqual(encoded.valueType, BamlTypeDescriptor.tuple([.int, .string, .int]).raw)
        XCTAssertEqual(encoded.listValue.values.map(\.intValue), [1, 0, 2])
        XCTAssertEqual(encoded.listValue.values[1].stringValue, "a")

        XCTAssertEqual(try Named3._bamlDecode(outbound([int(1), string("a"), int(2)])), value)
        XCTAssertThrowsError(try Named3._bamlDecode(outbound([int(1), string("a")])))
        XCTAssertThrowsError(
            try Named3._bamlDecode(outbound([int(1), string("a"), int(2), int(3)]))
        )
        XCTAssertThrowsError(try Named3._bamlDecode(outbound([string("a"), int(1), int(2)])))
        var list = BamlBridge_Cffi_V1_BamlValueList()
        list.items = [int(1), string("a"), int(2)]
        var listValue = BamlBridge_Cffi_V1_BamlOutboundValue()
        listValue.listValue = list
        XCTAssertThrowsError(try Named3._bamlDecode(BamlOutboundValue(listValue)))
    }

    /// A generated tuple struct is registered by its tuple type: a union
    /// takes it as the arm the value's carried type spells, ahead of an
    /// earlier arm the elements also fit, and never by its name.
    func testUnionPicksTheGeneratedTupleStructByCarriedType() throws {
        typealias Arms = BamlUnion2<BamlTuple3<Int, String?, Int>, Named3>
        let items = [int(1), string("a"), int(2)]
        let carried = outbound(items, carrying: Named3._bamlStatedType)
        XCTAssertEqual(try Arms._bamlDecode(carried), .t1(Named3(t0: 1, t1: "a", t2: 2)))
        XCTAssertEqual(
            try Arms._bamlDecode(carried, as: Arms._bamlSlot),
            .t1(Named3(t0: 1, t1: "a", t2: 2))
        )
        let open = outbound(items, carrying: BamlTuple3<Int, String?, Int>._bamlStatedType)
        XCTAssertEqual(try Arms._bamlDecode(open), .t0(BamlTuple3(1, "a", 2)))
    }

    // MARK: - The dynamic `unknown[]` stage

    /// The last stage, an array of the dynamic value: each element stays as
    /// it arrived (a tuple element keeps its carried type), the arity is
    /// checked both ways, and sending it back restates every element under
    /// the types it carries inside a list stating the declared tuple.
    func testDynamicTupleListKeepsEachElementAsItArrived() throws {
        let pair = BamlTypeDescriptor.tuple([.union([.int, .string]), .bool])
        let slot = BamlSlots.tupleList(BamlSlot<BamlOutboundValue>.own, slots: .int, pair)
        var flag = BamlBridge_Cffi_V1_BamlOutboundValue()
        flag.boolValue = true
        let nested = outbound([int(1), flag], carrying: pair)
        let value = outbound([int(7), nested.raw])

        let decoded = try [BamlOutboundValue]._bamlDecode(value, as: slot)
        XCTAssertEqual(decoded.count, 2)
        XCTAssertEqual(try Int._bamlDecode(decoded[0]), 7)
        XCTAssertEqual(decoded[1].carriedTupleType(), pair)
        XCTAssertThrowsError(
            try [BamlOutboundValue]._bamlDecode(outbound([int(7)]), as: slot)
        )

        let encoded = try decoded._bamlEncode(as: slot).raw
        XCTAssertEqual(encoded.valueType, BamlTypeDescriptor.tuple([.int, pair]).raw)
        XCTAssertEqual(encoded.listValue.values[0].intValue, 7)
        XCTAssertEqual(encoded.listValue.values[1].valueType, pair.raw)
        XCTAssertThrowsError(try Array(decoded.prefix(1))._bamlEncode(as: slot))
    }

    /// A dynamic value the host cannot state back is the bridge's failure.
    func testDynamicValueWithoutItsTypeIsRefused() {
        XCTAssertThrowsError(try outbound([int(1)])._bamlEncode()) { error in
            XCTAssertTrue(error is BamlBridgeFailure, "\(error)")
        }
    }

    // MARK: - Ownership after a partial failure

    /// A tuple whose later slot fails to decode decodes each earlier slot
    /// once and keeps nothing: what they decoded is released with the
    /// failure.
    func testTupleDecodeFailingPartWayDecodesEachSlotOnceAndKeepsNothing() {
        Tracked.reset()
        XCTAssertThrowsError(
            try BamlTuple3<Tracked, Tracked, Int>._bamlDecode(
                outbound([int(1), int(2), string("not an int")])
            )
        )
        XCTAssertEqual(Tracked.decoded, 2)
        XCTAssertEqual(Tracked.live, 0)

    }

    /// A fresh engine handle key no Swift value owns, as a wire slot: a
    /// clone of a media handle the engine minted.
    private func unownedHandle() throws -> (key: UInt64, value: BamlBridge_Cffi_V1_BamlOutboundValue) {
        let media = try BamlMedia.fromBase64(.image, "aGk=", mimeType: "image/png")
        var key: UInt64 = 0
        XCTAssertEqual(BamlApi.handleClone(media.key, &key), BAML_CFFI_STATUS_OK.rawValue)
        var handle = BamlBridge_Cffi_V1_BamlOutboundHandle()
        handle.key = key
        handle.handleType = .adtRuntimeValue
        var value = BamlBridge_Cffi_V1_BamlOutboundValue()
        value.handleValue = handle
        return (key, value)
    }

    /// Whether the engine still holds `key`: only a live key clones.
    private func isLive(_ key: UInt64) -> Bool {
        var clone: UInt64 = 0
        guard BamlApi.handleClone(key, &clone) == BAML_CFFI_STATUS_OK.rawValue else { return false }
        _ = BamlApi.handleRelease(clone)
        return true
    }

    /// A tuple whose slot fails to decode releases the engine handles of the
    /// slots after it, nested ones included: no value adopted them, and the
    /// wire value that carried them has no destructor.
    func testTupleDecodeFailureReleasesTheHandlesOfLaterSlots() throws {
        let later = try unownedHandle()
        let nested = try unownedHandle()
        let value = outbound([int(1), string("not an int"), later.value, outbound([nested.value]).raw])
        XCTAssertTrue(isLive(later.key) && isLive(nested.key))
        XCTAssertThrowsError(
            try BamlTuple4<Int, Int, BamlHandle, BamlTuple1<BamlHandle>>._bamlDecode(value)
        )
        XCTAssertFalse(isLive(later.key))
        XCTAssertFalse(isLive(nested.key))

        // Under a declared slot too.
        let declared = try unownedHandle()
        typealias Pair = BamlTuple2<Int, BamlOutboundValue>
        let slot = BamlSlots.tuple(BamlSlot<Int>.own, BamlSlot<BamlOutboundValue>.own)
        XCTAssertThrowsError(try Pair._bamlDecode(outbound([string("x"), declared.value]), as: slot))
        XCTAssertFalse(isLive(declared.key))

        // A generated tuple struct's decoder does the same.
        let generated = try unownedHandle()
        XCTAssertThrowsError(try Named3._bamlDecode(outbound([string("x"), string("a"), generated.value])))
        XCTAssertFalse(isLive(generated.key))
    }

    /// A slot decoded before the failing one keeps its own handle, given back
    /// once when the decoded value is dropped; a successful decode keeps
    /// every slot's handle.
    func testTupleDecodeKeepsTheHandlesItsSlotsAdopted() throws {
        let earlier = try unownedHandle()
        XCTAssertThrowsError(
            try BamlTuple2<BamlHandle, Int>._bamlDecode(outbound([earlier.value, string("x")]))
        )
        XCTAssertFalse(isLive(earlier.key))

        let kept = try unownedHandle()
        let decoded = try BamlTuple2<Int, BamlHandle>._bamlDecode(outbound([int(1), kept.value]))
        XCTAssertTrue(isLive(kept.key))
        withExtendedLifetime(decoded) {}
    }

    /// A tuple whose later slot fails to encode encodes each earlier slot
    /// once and surfaces that slot's own error.
    func testTupleEncodeFailingPartWayEncodesEachSlotOnce() {
        Probe.encoded = 0
        let slot = BamlSlots.tuple(BamlSlot<Probe>.own, BamlSlot<Probe>.own, BamlSlot<Failing>.own)
        XCTAssertThrowsError(try BamlTuple3(Probe(), Probe(), Failing())._bamlEncode(as: slot)) {
            XCTAssertTrue($0 is Failing.Failure, "\($0)")
        }
        XCTAssertEqual(Probe.encoded, 2)
    }
}

/// Shaped exactly like a generated `TupleN_...` struct (written in the same
/// entry points), at a width small enough to read.
private struct Named3: Equatable, Sendable, BamlTyped, BamlDecodable {
    let t0: Int
    let t1: String
    let t2: Int

    static var _bamlSlot: BamlSlot<Self> {
        ._tupleType(BamlSlot<Int>.own, BamlSlot<String>.own, BamlSlot<Int>.own)
    }

    func _bamlEncode(as slot: BamlSlot<Self>) throws -> BamlInboundValue {
        try .baml_tuple(slot, [
            { try self.t0._bamlEncode(as: slot._tupleItem(0)) },
            { try self.t1._bamlEncode(as: slot._tupleItem(1)) },
            { try self.t2._bamlEncode(as: slot._tupleItem(2)) },
        ])
    }

    static func _bamlDecode(_ v: BamlOutboundValue) throws -> Self {
        try _bamlDecode(v, as: _bamlSlot)
    }

    static func _bamlDecode(_ v: BamlOutboundValue, as slot: BamlSlot<Self>) throws -> Self {
        let items = try v._tupleItems(arity: 3)
        return Self(
            t0: try items._tupleSlot(0, as: slot._tupleItem(0)),
            t1: try items._tupleSlot(1, as: slot._tupleItem(1)),
            t2: try items._tupleSlot(2, as: slot._tupleItem(2))
        )
    }
}

/// A decoded value that counts its decodes and its live instances.
private final class Tracked: BamlDecodable {
    nonisolated(unsafe) static var decoded = 0
    nonisolated(unsafe) static var live = 0

    static func reset() {
        decoded = 0
        live = 0
    }

    init() { Self.live += 1 }
    deinit { Self.live -= 1 }

    static func _bamlDecode(_ value: BamlOutboundValue) throws -> Tracked {
        _ = try Int._bamlDecode(value)
        decoded += 1
        return Tracked()
    }
}

/// A leaf that counts its encodes.
private struct Probe: BamlLeafEncodable {
    nonisolated(unsafe) static var encoded = 0

    func _bamlEncode(as _: BamlSlot<Probe>) throws -> BamlInboundValue {
        Self.encoded += 1
        return try (1 as Int)._bamlEncode()
    }
}

extension BamlSlot where Value == Probe {
    fileprivate static var own: BamlSlot<Probe> { ._leaf(.int) }
}

/// A leaf that cannot be encoded.
private struct Failing: BamlLeafEncodable {
    struct Failure: Error {}

    func _bamlEncode(as _: BamlSlot<Failing>) throws -> BamlInboundValue {
        throw Failure()
    }
}

extension BamlSlot where Value == Failing {
    fileprivate static var own: BamlSlot<Failing> { ._leaf(.int) }
}
