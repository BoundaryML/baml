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
    /// round-trip through the inbound envelope.
    func testProtoRoundTrip() throws {
        var value = BamlBridge_Cffi_V1_InboundValue()
        value.valueType = BamlTypeDescriptor.int.raw
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
        XCTAssertTrue(decoded.kwargs.first?.value.hasValueType == true)
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

        let stream = try BamlStream<String, String>._bamlDecode(BamlOutboundValue(raw))
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
            try BamlStream<String, String>._bamlDecode(BamlOutboundValue(raw))
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

    func testClassIdentityUsesNodeLocalValueType() {
        let encoded = BamlInboundValue.baml_class(
            "user.Box",
            typeArguments: [.int],
            []
        ).raw

        guard case .classTy(let classType)? = encoded.valueType.ty else {
            return XCTFail("expected a class value_type")
        }
        XCTAssertEqual(classType.name, "user.Box")
        XCTAssertEqual(classType.typeArgs.first?.primitive.kind, .bamlTyPrimitiveInt)
        guard case .classValue(let payload)? = encoded.value else {
            return XCTFail("expected a class payload")
        }
        XCTAssertTrue(payload.fields.isEmpty)
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

    func testTyDefReportsUnsupportedReflection() {
        var raw = BamlBridge_Cffi_V1_BamlOutboundValue()
        raw.tyDefValue = BamlBridge_Cffi_V1_BamlTyDef()

        XCTAssertThrowsError(
            try Int._bamlDecode(BamlOutboundValue(raw))
        ) { error in
            guard case BamlDecodeError.typeMismatch(let expected, let got) = error else {
                return XCTFail(
                    "expected BamlDecodeError.typeMismatch, got \(error)"
                )
            }
            XCTAssertEqual(expected, "Int")
            XCTAssertTrue(got.contains("requires BEP-066 reflection support"))
            XCTAssertTrue(got.contains("Swift SDK does not provide"))
        }
    }
}
