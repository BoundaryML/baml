import Foundation
import XCTest
import CBamlBridge

@testable import BamlBridge

final class TupleCodecTests: XCTestCase {
    private typealias Pair = (Int, String)

    private let pair = BamlSlots.tuple(BamlSlot<Int>.own, BamlSlot<String>.own)

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

    private func list(_ items: [BamlBridge_Cffi_V1_BamlOutboundValue]) -> BamlOutboundValue {
        var list = BamlBridge_Cffi_V1_BamlValueList()
        list.items = items
        var value = BamlBridge_Cffi_V1_BamlOutboundValue()
        value.listValue = list
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

    private func bool(_ value: Bool) -> BamlBridge_Cffi_V1_BamlOutboundValue {
        var out = BamlBridge_Cffi_V1_BamlOutboundValue()
        out.boolValue = value
        return out
    }

    private func echoed(_ value: BamlInboundValue) -> BamlOutboundValue {
        func echo(_ value: BamlBridge_Cffi_V1_InboundValue) -> BamlBridge_Cffi_V1_BamlOutboundValue {
            var out = BamlBridge_Cffi_V1_BamlOutboundValue()
            switch value.value {
            case .intValue(let v): out.intValue = v
            case .stringValue(let v): out.stringValue = v
            case .boolValue(let v): out.boolValue = v
            case .listValue(let list):
                let items = list.values.map(echo)
                if case .tuple? = value.valueType.ty {
                    out.tupleValue.items = items
                    out.tupleValue.tupleType = value.valueType
                } else {
                    out.listValue.items = items
                }
            case .mapValue(let map):
                out.mapValue.entries = map.entries.map { entry in
                    var echoed = BamlBridge_Cffi_V1_BamlOutboundMapEntry()
                    echoed.key = entry.stringKey
                    echoed.value = echo(entry.value)
                    return echoed
                }
            case nil: break
            default: XCTFail("unexpected inbound value \(value)")
            }
            return out
        }
        return BamlOutboundValue(echo(value.raw))
    }


    func testTupleStatesItsSlotTypesInOrder() throws {
        let encoded = try pair._bamlEncode((1, "a")).raw
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
        XCTAssertNotEqual(pair.type, BamlSlots.tuple(BamlSlot<String>.own, BamlSlot<Int>.own).type)
    }

    func testTupleSlotStatesItsDeclaredType() throws {
        let slot = BamlSlots.tuple(BamlSlot<String>.literal("a"), BamlSlot<Int>.own)
        let encoded = try slot._bamlEncode(("a", 1)).raw
        guard case .tuple(let tuple)? = encoded.valueType.ty else {
            return XCTFail("expected a tuple value_type")
        }
        XCTAssertEqual(tuple.items[0].literal.stringValue, "a")
    }

    func testTupleDecodesItsSlots() throws {
        let decoded: Pair = try pair._bamlDecode(outbound([int(1), string("a")]))
        XCTAssertEqual(decoded.0, 1)
        XCTAssertEqual(decoded.1, "a")
        XCTAssertTrue(pair._bamlEqual(decoded, (1, "a")))
        XCTAssertFalse(pair._bamlEqual(decoded, (1, "b")))
    }

    func testTupleRejectsWrongArityAndWrongSlot() {
        XCTAssertThrowsError(try pair._bamlDecode(outbound([int(1)])))
        XCTAssertThrowsError(try pair._bamlDecode(outbound([int(1), string("a"), int(2)])))
        XCTAssertThrowsError(try pair._bamlDecode(outbound([string("a"), int(1)])))
        XCTAssertThrowsError(try BamlSlots.tuple(BamlSlot<Int>.own)._bamlDecode(outbound([int(1), string("a")])))
    }

    func testListAndTupleDoNotStandForEachOther() {
        XCTAssertThrowsError(try pair._bamlDecode(list([int(1), string("a")])))
        XCTAssertThrowsError(try BamlSlots.tuple(BamlSlot<Int>.own)._bamlDecode(list([int(1)])))
        XCTAssertThrowsError(try [Int]._bamlDecode(outbound([int(1), int(2)])))
    }

    func testWideTupleRoundTrips() throws {
        let slot = BamlSlots.tuple(
            BamlSlot<Int>.own, BamlSlot<String>.own, BamlSlot<Int>.own, BamlSlot<String>.own,
            BamlSlot<Int>.own, BamlSlot<String>.own, BamlSlot<Int>.own, BamlSlot<String>.own,
            BamlSlot<Int>.own, BamlSlot<String>.own, BamlSlot<Int>.own, BamlSlot<String>.own,
            BamlSlot<Int>.own, BamlSlot<Bool>.own
        )
        let value = (0, "1", 2, "3", 4, "5", 6, "7", 8, "9", 10, "11", 12, true)
        let encoded = try slot._bamlEncode(value)
        XCTAssertEqual(encoded.raw.listValue.values.count, 14)
        XCTAssertEqual(encoded.raw.valueType.tuple.items.count, 14)
        let decoded = try slot._bamlDecode(echoed(encoded))
        XCTAssertTrue(slot._bamlEqual(decoded, value))
        XCTAssertEqual(decoded.13, true)
        XCTAssertEqual(decoded.11, "11")
        XCTAssertThrowsError(try slot._bamlDecode(outbound(Array(repeating: int(1), count: 13))))
    }

    func testNestedTuplesAndContainersRoundTrip() throws {
        let nested = BamlSlots.tuple(pair, BamlSlot<Bool>.own)
        let value = ((1, "a"), true)
        let encoded = try nested._bamlEncode(value)
        XCTAssertEqual(
            encoded.raw.valueType,
            BamlTypeDescriptor.tuple([.tuple([.int, .string]), .bool]).raw
        )
        XCTAssertTrue(nested._bamlEqual(try nested._bamlDecode(echoed(encoded)), value))
        XCTAssertFalse(nested._bamlEqual(value, ((1, "a"), false)))

        let pairs = BamlSlots.list(BamlSlots.tuple(BamlSlot<String>.own, BamlSlot<Int>.own))
        let xs = [("a", 1), ("b", 2)]
        XCTAssertTrue(pairs._bamlEqual(try pairs._bamlDecode(echoed(try pairs._bamlEncode(xs))), xs))
        XCTAssertTrue(pairs._bamlEqual(try pairs._bamlDecode(echoed(try pairs._bamlEncode([]))), []))
        XCTAssertFalse(pairs._bamlEqual(xs, [("a", 1)]))

        let map = BamlSlots.map(BamlSlots.tuple(BamlSlot<Int>.own, BamlSlot<Bool>.own))
        let m = ["k": (1, true), "j": (2, false)]
        XCTAssertTrue(map._bamlEqual(try map._bamlDecode(echoed(try map._bamlEncode(m))), m))
        XCTAssertFalse(map._bamlEqual(m, ["k": (1, true), "j": (2, true)]))

        let optional = BamlSlots.optional(
            BamlSlots.tuple(BamlSlot<Int>.own, BamlSlots.optional(BamlSlot<String>.own))
        )
        for value: (Int, String?)? in [(1, "s"), (1, nil), nil] {
            let decoded = try optional._bamlDecode(echoed(try optional._bamlEncode(value)))
            XCTAssertTrue(optional._bamlEqual(decoded, value), "\(String(describing: value))")
        }

        let single = BamlSlots.tuple(pair)
        let wrapped = Tuple1((3, "c"))
        let roundTripped = try single._bamlDecode(echoed(try single._bamlEncode(wrapped)))
        XCTAssertTrue(single._bamlEqual(roundTripped, wrapped))
        XCTAssertEqual(roundTripped.item0.1, "c")
    }

    func testTupleNestingCountsTowardTheWireBound() throws {
        func nested(_ depth: Int) -> () throws -> BamlInboundValue {
            var type = BamlTypeDescriptor.int
            var encode: () throws -> BamlInboundValue = { try BamlSlot<Int>.own._bamlEncode(0) }
            for _ in 0..<depth {
                let inner = encode
                type = .tuple([type])
                let stated = type
                encode = { try BamlInboundValue.tupleNode(stating: stated, [inner]) }
            }
            return encode
        }
        XCTAssertNoThrow(try nested(BamlValueNesting.maxDepth)())
        XCTAssertThrowsError(try nested(BamlValueNesting.maxDepth + 1)())
    }

    func testTupleCrossesByCopy() throws {
        var value: Pair = (1, "a")
        let encoded = try pair._bamlEncode(value)
        value.0 = 2
        value.1 = "b"
        XCTAssertEqual(encoded.raw.listValue.values[0].intValue, 1)
        XCTAssertEqual(encoded.raw.listValue.values[1].stringValue, "a")

        let received = outbound([int(1), string("a")])
        var decoded = try pair._bamlDecode(received)
        decoded.0 = 9
        XCTAssertEqual(decoded.0, 9)
        XCTAssertEqual(try pair._bamlDecode(received).0, 1)
    }


    func testTuple1IsAOneSlotTuple() throws {
        let slot = BamlSlots.tuple(BamlSlot<String>.own)
        XCTAssertEqual(slot.type, .tuple([.string]))
        XCTAssertEqual(Tuple1<String>._bamlStatedType, .tuple([.string]))

        let encoded = try Tuple1("x")._bamlEncode().raw
        XCTAssertEqual(encoded.valueType, BamlTypeDescriptor.tuple([.string]).raw)
        XCTAssertEqual(encoded.listValue.values.map(\.stringValue), ["x"])

        var decoded = try Tuple1<String>._bamlDecode(outbound([string("x")]))
        XCTAssertEqual(decoded, Tuple1("x"))
        XCTAssertEqual(try slot._bamlDecode(outbound([string("x")])), Tuple1("x"))
        XCTAssertThrowsError(try Tuple1<String>._bamlDecode(outbound([string("x"), string("y")])))
        XCTAssertThrowsError(try Tuple1<String>._bamlDecode(BamlOutboundValue({
            var value = BamlBridge_Cffi_V1_BamlOutboundValue()
            value.stringValue = "x"
            return value
        }())))

        decoded.item0 = "y"
        XCTAssertEqual(decoded.item0, "y")
        XCTAssertEqual(Set([Tuple1(1), Tuple1(1), Tuple1(2)]).count, 2)
        XCTAssertEqual(Tuple1(1).description, "(1,)")
    }


    func testUnionWithATupleArmRoundTrips() throws {
        let slot = BamlSlots.union(pair, BamlSlot<String>.own)
        let tuple: BamlUnion2<Pair, String> = .t0((1, "a"))
        let string: BamlUnion2<Pair, String> = .t1("s")
        for value in [tuple, string] {
            let decoded = try slot._bamlDecode(echoed(try slot._bamlEncode(value)))
            XCTAssertTrue(slot._bamlEqual(decoded, value))
        }
        XCTAssertFalse(slot._bamlEqual(tuple, string))
        XCTAssertFalse(slot._bamlEqual(tuple, .t0((1, "b"))))
        XCTAssertEqual(try slot._bamlEncode(tuple).raw.valueType, pair.type.raw)
    }

    func testUnionPicksTheTupleArmByCarriedType() throws {
        let slot = BamlSlots.union(
            pair,
            BamlSlots.tuple(BamlSlot<Int>.own, BamlSlots.optional(BamlSlot<String>.own))
        )
        let carried = BamlTypeDescriptor.tuple([.int, .optional(.string)])
        guard case .t1(let picked) = try slot._bamlDecode(outbound([int(1), string("a")], carrying: carried)) else {
            return XCTFail("expected the carried arm")
        }
        XCTAssertEqual(picked.0, 1)
        XCTAssertEqual(picked.1, "a")
        guard case .t0 = try slot._bamlDecode(outbound([int(1), string("a")])) else {
            return XCTFail("expected the first arm that fits")
        }
        XCTAssertNil(outbound([int(1)]).carriedTupleType())

        typealias Singles = BamlUnion2<Tuple1<Int>, Tuple1<Int?>>
        let single = outbound([int(1)], carrying: .tuple([.optional(.int)]))
        XCTAssertEqual(try Singles._bamlDecode(single), .t1(Tuple1(1)))
        XCTAssertEqual(try Singles._bamlDecode(single, as: Singles._bamlSlot), .t1(Tuple1(1)))
    }

    func testUnionPicksTheSelectedTupleArm() throws {
        let swapped = BamlSlots.tuple(BamlSlot<String>.own, BamlSlot<Int>.own)
        let slot = BamlSlots.union(pair, swapped)
        var variant = BamlBridge_Cffi_V1_BamlValueUnionVariant()
        variant.selfType = slot.type.raw
        variant.selectedOptionIndex = 1
        variant.value = outbound([string("a"), int(1)]).raw
        var value = BamlBridge_Cffi_V1_BamlOutboundValue()
        value.unionVariantValue = variant
        guard case .t1(let picked) = try slot._bamlDecode(BamlOutboundValue(value)) else {
            return XCTFail("expected the selected arm")
        }
        XCTAssertEqual(picked.0, "a")
    }


    func testThrownTupleFillsItsDeclaredSlot() throws {
        let thrown = BamlThrownSlot(pair)
        let encoded = try XCTUnwrap(try thrown.encode((7, "boom")))
        XCTAssertEqual(encoded.raw.valueType, pair.type.raw)
        XCTAssertNil(try thrown.encode(("boom", 7)))
    }


    func testReceivedTupleIsRestatedWithItsCarriedType() throws {
        let carried = BamlTypeDescriptor.tuple([.union([.int, .string]), .bool])
        let restated = try restate(outbound([int(1), bool(true)], carrying: carried).raw)
        XCTAssertEqual(restated.valueType, carried.raw)
        XCTAssertEqual(restated.listValue.values.count, 2)
        XCTAssertEqual(restated.listValue.values[0].intValue, 1)
        XCTAssertEqual(restated.listValue.values[1].boolValue, true)
    }

    func testReceivedTupleWithoutItsTypeIsRefused() {
        XCTAssertThrowsError(try restate(outbound([int(1), string("a")]).raw))
        XCTAssertThrowsError(
            try restate(outbound([int(1), string("a")], carrying: .tuple([.int])).raw)
        )
        XCTAssertThrowsError(
            try restate(outbound([int(1)], carrying: .list(.int)).raw)
        )
    }

    func testReceivedTupleNamingALiveDeclarationIsRefused() {
        var cls = BamlBridge_Cffi_V1_BamlTyClass()
        cls.live = 7
        var item = BamlBridge_Cffi_V1_BamlTy()
        item.classTy = cls
        let carried = BamlTypeDescriptor.tuple([BamlTypeDescriptor(item)])
        XCTAssertThrowsError(try restate(outbound([int(1)], carrying: carried).raw))
    }

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


    func testTupleDecodeFailingPartWayDecodesEachSlotOnceAndKeepsNothing() {
        Tracked.reset()
        let slot = BamlSlots.tuple(Tracked.slot, Tracked.slot, BamlSlot<Int>.own)
        XCTAssertThrowsError(
            try slot._bamlDecode(outbound([int(1), int(2), string("not an int")]))
        )
        XCTAssertEqual(Tracked.decoded, 2)
        XCTAssertEqual(Tracked.live, 0)
    }

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

    private func isLive(_ key: UInt64) -> Bool {
        var clone: UInt64 = 0
        guard BamlApi.handleClone(key, &clone) == BAML_CFFI_STATUS_OK.rawValue else { return false }
        _ = BamlApi.handleRelease(clone)
        return true
    }

    private var handleSlot: BamlSlot<BamlHandle> {
        BamlSlot(BamlSlotNode(type: .rustType, shape: .leaf))
    }

    func testTupleDecodeFailureReleasesTheHandlesOfLaterSlots() throws {
        let later = try unownedHandle()
        let nested = try unownedHandle()
        let value = outbound([int(1), string("not an int"), later.value, outbound([nested.value]).raw])
        XCTAssertTrue(isLive(later.key) && isLive(nested.key))
        let slot = BamlSlots.tuple(
            BamlSlot<Int>.own, BamlSlot<Int>.own, handleSlot, BamlSlots.tuple(handleSlot)
        )
        XCTAssertThrowsError(try slot._bamlDecode(value))
        XCTAssertFalse(isLive(later.key))
        XCTAssertFalse(isLive(nested.key))

        let single = try unownedHandle()
        XCTAssertThrowsError(
            try BamlSlots.tuple(BamlSlot<Int>.own)._bamlDecode(outbound([single.value]))
        )
        _ = BamlApi.handleRelease(single.key)
    }

    func testTupleDecodeKeepsTheHandlesItsSlotsAdopted() throws {
        let earlier = try unownedHandle()
        XCTAssertThrowsError(
            try BamlSlots.tuple(handleSlot, BamlSlot<Int>.own)._bamlDecode(outbound([earlier.value, string("x")]))
        )
        XCTAssertFalse(isLive(earlier.key))

        let kept = try unownedHandle()
        let decoded = try BamlSlots.tuple(BamlSlot<Int>.own, handleSlot)._bamlDecode(outbound([int(1), kept.value]))
        XCTAssertTrue(isLive(kept.key))
        withExtendedLifetime(decoded) {}
    }

    func testTupleEncodeFailingPartWayEncodesEachSlotOnce() {
        Probe.encoded = 0
        let slot = BamlSlots.tuple(BamlSlot<Probe>.own, BamlSlot<Probe>.own, BamlSlot<Failing>.own)
        XCTAssertThrowsError(try slot._bamlEncode((Probe(), Probe(), Failing()))) {
            XCTAssertTrue($0 is Failing.Failure, "\($0)")
        }
        XCTAssertEqual(Probe.encoded, 2)
    }
}

private final class Tracked: BamlDecodable {
    nonisolated(unsafe) static var decoded = 0
    nonisolated(unsafe) static var live = 0

    static var slot: BamlSlot<Tracked> { BamlSlot(BamlSlotNode(type: .int, shape: .leaf)) }

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

private struct Failing: BamlLeafEncodable {
    struct Failure: Error {}

    func _bamlEncode(as _: BamlSlot<Failing>) throws -> BamlInboundValue {
        throw Failure()
    }
}

extension BamlSlot where Value == Failing {
    fileprivate static var own: BamlSlot<Failing> { ._leaf(.int) }
}
