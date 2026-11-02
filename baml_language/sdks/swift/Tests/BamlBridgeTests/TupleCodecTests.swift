import Foundation
import XCTest

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
}
