import XCTest

@testable import BamlBridge

final class FFISmokeTests: XCTestCase {
    /// Proves the full link chain: SwiftPM → BamlBridgeFFI.xcframework →
    /// Rust staticlib → `version()` over the C ABI (including Buffer
    /// ownership: bytes copied out, then `free_buffer`).
    func testNativeVersionNonEmpty() {
        let v = BamlRuntime.nativeVersion()
        XCTAssertFalse(v.isEmpty, "native version() returned an empty buffer")
    }

    /// Sanity-check the checked-in swift-protobuf sources: a wire
    /// round-trip through the inbound envelope. A scalar's arm implies its
    /// type, so it carries no `value_type` (the decoder refuses one).
    func testProtoRoundTrip() throws {
        var value = BamlBridge_Cffi_V1_InboundValue()
        value.intValue = 42
        var entry = BamlBridge_Cffi_V1_InboundMapEntry()
        entry.stringKey = "x"
        entry.value = value
        var args = BamlBridge_Cffi_V1_CallFunctionArgs()
        args.kwargs = [entry]
        args.callID = 7

        let bytes = try args.serializedData()
        let decoded = try BamlBridge_Cffi_V1_CallFunctionArgs(serializedBytes: bytes)
        XCTAssertEqual(decoded.callID, 7)
        XCTAssertEqual(decoded.kwargs.first?.stringKey, "x")
        XCTAssertEqual(decoded.kwargs.first?.value.intValue, 42)
        XCTAssertTrue(decoded.kwargs.first?.value.hasValueType == false)
    }

    /// A generic call's type arguments ride `CallFunctionArgs.type_args`, in
    /// the order given, beside the arguments encoded under the same types.
    func testCallArgsCarryTypeArguments() throws {
        let bytes = try encodeCallArgs(
            [BamlArgument("xs", [Int](), as: BamlSlots.list(BamlSlot<Int>.own))],
            typeArgs: [
                BamlTypeArgument("T", BamlSlot<Int>.own.type),
                BamlTypeArgument("U", .list(.string)),
            ],
            callId: 7,
            callTarget: .functionName("first_or")
        )
        let decoded = try BamlBridge_Cffi_V1_CallFunctionArgs(serializedBytes: bytes)
        XCTAssertEqual(decoded.typeArgs.map(\.typeVar), ["T", "U"])
        XCTAssertEqual(decoded.typeArgs[0].typeValue, BamlTypeDescriptor.int.raw)
        XCTAssertEqual(decoded.typeArgs[1].typeValue, BamlTypeDescriptor.list(.string).raw)
        XCTAssertEqual(decoded.kwargs.first?.value.valueType, BamlTypeDescriptor.list(.int).raw)

        let plain = try BamlBridge_Cffi_V1_CallFunctionArgs(
            serializedBytes: encodeCallArgs([], callId: 7, callTarget: .functionName("f"))
        )
        XCTAssertTrue(plain.typeArgs.isEmpty)
    }

    func testBigIntPreservesArbitraryPrecisionWireValues() throws {
        let value = try BamlBigInt(hexadecimal: "18ee90ff6c373e0ee4e3f0ad2")
        let encoded = value._bamlEncode().raw
        XCTAssertEqual(encoded.bigintValue, value.hexadecimal)
        XCTAssertFalse(encoded.hasValueType)
        XCTAssertEqual(BamlBigInt._bamlStatedType.raw.primitive.kind, .bamlTyPrimitiveBigint)

        var raw = BamlBridge_Cffi_V1_BamlOutboundValue()
        raw.bigintValue = value.hexadecimal
        XCTAssertEqual(try BamlBigInt._bamlDecode(BamlOutboundValue(raw)), value)
        XCTAssertThrowsError(try BamlBigInt(hexadecimal: "+a"))
        XCTAssertThrowsError(try BamlBigInt(hexadecimal: "00"))
        XCTAssertThrowsError(try BamlBigInt(hexadecimal: "-0"))
        XCTAssertThrowsError(try BamlBigInt(hexadecimal: "A"))
    }

    func testPromptJSONRejectsDuplicateMapKeys() {
        var value = BamlBridge_Cffi_V1_BamlOutboundValue()
        value.stringValue = "value"
        var first = BamlBridge_Cffi_V1_BamlOutboundMapEntry()
        first.key = "duplicate"
        first.value = value
        var second = BamlBridge_Cffi_V1_BamlOutboundMapEntry()
        second.key = "duplicate"
        second.value = value
        var map = BamlBridge_Cffi_V1_BamlValueMap()
        map.entries = [first, second]
        var raw = BamlBridge_Cffi_V1_BamlOutboundValue()
        raw.mapValue = map

        XCTAssertThrowsError(try BamlPromptJSON._bamlDecode(BamlOutboundValue(raw)))
    }

    func testStreamHandleRetainsCarriedClassIdentity() throws {
        var classType = BamlBridge_Cffi_V1_BamlTyClass()
        classType.name = "ai.stream.Stream"
        classType.typeArgs = [BamlTypeDescriptor.string.raw, BamlTypeDescriptor.string.raw]
        var ty = BamlBridge_Cffi_V1_BamlTy()
        ty.classTy = classType

        var handle = BamlBridge_Cffi_V1_BamlOutboundHandle()
        handle.key = 101
        handle.handleType = .adtTaggedHeapHandle
        handle.ty = ty
        var raw = BamlBridge_Cffi_V1_BamlOutboundValue()
        raw.handleValue = handle

        let stream = try BamlStream<String>._bamlDecode(BamlOutboundValue(raw))
        XCTAssertEqual(stream.bamlClassFQN, "ai.stream.Stream")
        XCTAssertEqual(stream.nextFQN, "ai.stream.Stream.next")
        XCTAssertEqual(stream.finalFQN, "ai.stream.Stream.final")
    }

    func testStreamHandleRequiresCarriedClassIdentity() {
        var handle = BamlBridge_Cffi_V1_BamlOutboundHandle()
        handle.key = 102
        handle.handleType = .adtTaggedHeapHandle
        var raw = BamlBridge_Cffi_V1_BamlOutboundValue()
        raw.handleValue = handle

        XCTAssertThrowsError(
            try BamlStream<String>._bamlDecode(BamlOutboundValue(raw))
        )
    }

    /// A static Swift union knows which empty-list arm was constructed. The
    /// selected child carries that exact list type while the payload stays a
    /// bare list value; no recursive scan of runtime entries is needed.
    func testEmptyListUnionCarriesSelectedArmType() {
        let value: BamlUnion2<[Int], [String]> = .t1([])
        let encoded = value._bamlEncode().raw

        guard case .list(let listType)? = encoded.valueType.ty else {
            return XCTFail("expected a list value_type")
        }
        XCTAssertEqual(listType.item.primitive.kind, .bamlTyPrimitiveString)
        guard case .listValue(let list)? = encoded.value else {
            return XCTFail("expected a bare list payload")
        }
        XCTAssertTrue(list.values.isEmpty)
    }

    /// A Swift array or dictionary knows its element type statically and
    /// states it, so an empty container or one under an `unknown` or generic
    /// parameter leaves the engine nothing to infer.
    func testContainersStateTheirStaticElementType() {
        let list = [Int]()._bamlEncode().raw
        guard case .list(let listType)? = list.valueType.ty else {
            return XCTFail("expected a list value_type")
        }
        XCTAssertEqual(listType.item.primitive.kind, .bamlTyPrimitiveInt)

        let map = ["a": "b"]._bamlEncode().raw
        guard case .map(let mapType)? = map.valueType.ty else {
            return XCTFail("expected a map value_type")
        }
        XCTAssertEqual(mapType.key.primitive.kind, .bamlTyPrimitiveString)
        XCTAssertEqual(mapType.value.primitive.kind, .bamlTyPrimitiveString)
    }

    func testClassIdentityUsesNodeLocalValueType() {
        let slot = BamlSlot<Box<Int>>.own
        let encoded = Box(value: 1)._bamlEncode(as: slot).raw

        guard case .classTy(let classType)? = encoded.valueType.ty else {
            return XCTFail("expected a class value_type")
        }
        XCTAssertEqual(classType.name, "user.Box")
        XCTAssertEqual(classType.typeArgs.first?.primitive.kind, .bamlTyPrimitiveInt)
        guard case .classValue(let payload)? = encoded.value else {
            return XCTFail("expected a class payload")
        }
        XCTAssertEqual(payload.fields.first?.value.intValue, 1)
    }

    /// `("draft" | "sent")[]` is a `[String]` in Swift; the list still states
    /// the literal union its slot declares, never `string[]`.
    func testLiteralListStatesTheDeclaredLiteralUnion() {
        let slot = BamlSlots.list(BamlSlot<String>.oneOf(.literal("draft"), .literal("sent")))
        let encoded = ["draft"]._bamlEncode(as: slot).raw

        guard case .list(let listType)? = encoded.valueType.ty,
              case .union(let union)? = listType.item.ty
        else {
            return XCTFail("expected a list of a union, got \(encoded.valueType)")
        }
        XCTAssertEqual(union.options.map(\.literal.stringValue), ["draft", "sent"])
        guard case .listValue(let list)? = encoded.value else {
            return XCTFail("expected a list payload")
        }
        XCTAssertFalse(list.values[0].hasValueType)
    }

    /// `Box<"a">` states its literal type argument, and the field typed by
    /// that argument is encoded under it: a `T[]` field states `"a"[]`.
    func testClassStatesLiteralTypeArgumentsAndPassesThemToFields() {
        let slot = BamlSlot<ListBox<String>>._classType("user.ListBox", BamlSlot<String>.literal("a"))
        let encoded = ListBox(items: ["a"])._bamlEncode(as: slot).raw

        guard case .classTy(let classType)? = encoded.valueType.ty else {
            return XCTFail("expected a class value_type")
        }
        XCTAssertEqual(classType.typeArgs.first?.literal.stringValue, "a")
        guard case .classValue(let payload)? = encoded.value,
              case .list(let fieldType)? = payload.fields.first?.value.valueType.ty
        else {
            return XCTFail("expected the field to state a list type")
        }
        XCTAssertEqual(fieldType.item.literal.stringValue, "a")
    }

    /// Media is the type `image`/`audio`/..., in the descriptor numbering
    /// (which orders `video` and `pdf` unlike the C ABI's media kinds).
    func testMediaDescriptorsSpellTheMediaType() {
        let kinds: [(BamlMedia.Kind, BamlBridge_Cffi_V1_BamlTyMediaKind)] = [
            (.image, .image), (.audio, .audio), (.video, .video), (.pdf, .pdf),
        ]
        for (kind, expected) in kinds {
            let list = BamlSlots.list(BamlSlot<String>._leaf(.media(kind))).type.raw
            guard case .list(let listType)? = list.ty, case .media(let media)? = listType.item.ty else {
                return XCTFail("expected a list of media for \(kind)")
            }
            XCTAssertEqual(media.kind, expected)
        }
    }

    /// A null-bearing recursive alias (`json`) is referenced as `Alias?`; a
    /// list of it states `json[]`, its `nil` elements being the alias's own
    /// null, never `(json | null)[]`.
    func testNullBearingAliasListStatesTheAlias() {
        let encoded = [JsonLike?.none, .number(1)]._bamlEncode().raw
        guard case .list(let listType)? = encoded.valueType.ty,
              case .typeAlias(let alias)? = listType.item.ty
        else {
            return XCTFail("expected json[], got \(encoded.valueType)")
        }
        XCTAssertEqual(alias.name, "baml.json.json")
    }

    /// `int?[]` states its optional element type.
    func testOptionalElementsStateTheirNull() {
        let encoded = [Int?.none, 1]._bamlEncode().raw
        guard case .list(let listType)? = encoded.valueType.ty,
              case .optional(let optional)? = listType.item.ty
        else {
            return XCTFail("expected int?[], got \(encoded.valueType)")
        }
        XCTAssertEqual(optional.inner.primitive.kind, .bamlTyPrimitiveInt)
    }

    /// The raw engine union contains a null hole that the compact Swift wrapper
    /// omits. Index 2 must first resolve through self_type to int[] and only then
    /// select t1; applying index 2 directly to BamlUnion2 would be out of range.
    func testOutboundUnionResolvesSelectedTypeThroughSelfType() throws {
        var payload = BamlBridge_Cffi_V1_BamlOutboundValue()
        payload.listValue = BamlBridge_Cffi_V1_BamlValueList()

        var union = BamlBridge_Cffi_V1_BamlValueUnionVariant()
        union.value = payload
        union.selfType = BamlTypeDescriptor.union([
            .null,
            .list(.string),
            .list(.int),
        ]).raw
        union.valueOptionName = "string[]"
        union.selectedOptionIndex = 2

        var wrapped = BamlBridge_Cffi_V1_BamlOutboundValue()
        wrapped.unionVariantValue = union
        let decoded = try BamlUnion2<[String], [Int]>._bamlDecode(
            BamlOutboundValue(wrapped)
        )

        guard case .t1(let ints) = decoded else {
            return XCTFail("selected type should select the compact int[] arm")
        }
        XCTAssertEqual(ints, [])
    }

    /// A type value has no Swift decodable: decoding one is a mismatch that
    /// names it.
    func testTypeValueIsRefused() {
        var cls = BamlBridge_Cffi_V1_BamlTyClass()
        cls.name = "user.Box"
        var raw = BamlBridge_Cffi_V1_BamlOutboundValue()
        raw.tyValue.classTy = cls

        XCTAssertThrowsError(try Int._bamlDecode(BamlOutboundValue(raw))) { error in
            guard case BamlDecodeError.typeMismatch(let expected, let got) = error else {
                return XCTFail("expected BamlDecodeError.typeMismatch, got \(error)")
            }
            XCTAssertEqual(expected, "Int")
            XCTAssertEqual(got, "type reference")
        }
    }

    /// Every `live` head is one reference, so each occurrence is found,
    /// through every arm that nests a type; `name` heads are not references.
    func testLiveKeysFindsEveryOccurrence() {
        var param = BamlBridge_Cffi_V1_BamlTyFunctionParam()
        param.ty.classTy.head = .live(1)
        var binding = BamlBridge_Cffi_V1_BamlTyAssociatedBinding()
        binding.ty.enumVariant.head = .live(3)
        var interface = BamlBridge_Cffi_V1_BamlTyInterface()
        interface.head = .live(2)
        interface.bindings = [binding]
        var function = BamlBridge_Cffi_V1_BamlTyFunction()
        function.params = [param]
        function.ret.interface = interface
        function.throws.future.value.typeAlias.name = "user.Alias"
        function.throws.future.error.list.item.enum.head = .live(1)
        var ty = BamlBridge_Cffi_V1_BamlTy()
        ty.function = function

        XCTAssertEqual(liveKeys(ty), [1, 2, 3, 1])
    }

}

/// Hand-written stand-ins for generated models, shaped as sdkgen_swift emits
/// them.
private struct Box<T: BamlCodableValue>: BamlTyped {
    var value: T

    static var _bamlSlot: BamlSlot<Self> { ._classType("user.Box", BamlSlot<T>.own) }

    func _bamlEncode(as slot: BamlSlot<Self>) -> BamlInboundValue {
        let argument0: BamlSlot<T> = slot._classArgument(0)
        return .baml_class(slot, [BamlField("value", self.value, as: argument0)])
    }
}

private struct ListBox<T: BamlCodableValue>: BamlTyped {
    var items: [T]

    static var _bamlSlot: BamlSlot<Self> { ._classType("user.ListBox", BamlSlot<T>.own) }

    func _bamlEncode(as slot: BamlSlot<Self>) -> BamlInboundValue {
        let argument0: BamlSlot<T> = slot._classArgument(0)
        return .baml_class(slot, [BamlField("items", self.items, as: BamlSlots.list(argument0))])
    }
}

private enum JsonLike: BamlTyped, BamlLeafEncodable {
    case number(Int)

    static var _bamlSlot: BamlSlot<Self> { ._leaf(.typeAlias("baml.json.json")) }
    static var _bamlOptionalSlot: BamlSlot<Self?> { BamlSlots.nullable(_bamlSlot) }

    func _bamlEncode(as _: BamlSlot<Self>) -> BamlInboundValue {
        switch self {
        case .number(let value): return value._bamlEncode(as: BamlSlot<Int>.own)
        }
    }
}
