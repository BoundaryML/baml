import XCTest

@testable import BamlBridge

private struct FirstIntArm: BamlDecodable, Equatable {
    let value: Int

    static func _bamlDecode(_ value: BamlOutboundValue) throws -> FirstIntArm {
        FirstIntArm(value: try Int._bamlDecode(value))
    }
}

private struct SecondIntArm: BamlDecodable, Equatable {
    let value: Int

    static func _bamlDecode(_ value: BamlOutboundValue) throws -> SecondIntArm {
        SecondIntArm(value: try Int._bamlDecode(value))
    }
}

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

    /// Both test arms accept the same integer payload, so structural fallback
    /// would always choose the first. The canonical index must select t1 even
    /// when legacy display metadata is misleading.
    func testOutboundUnionUsesSelectedOptionIndex() throws {
        var payload = BamlBridge_Cffi_V1_BamlOutboundValue()
        payload.intValue = 42

        var union = BamlBridge_Cffi_V1_BamlValueUnionVariant()
        union.value = payload
        union.valueOptionName = "FirstIntArm"
        union.selectedOptionIndex = 1

        var wrapped = BamlBridge_Cffi_V1_BamlOutboundValue()
        wrapped.unionVariantValue = union
        let decoded = try BamlUnion2<FirstIntArm, SecondIntArm>._bamlDecode(
            BamlOutboundValue(wrapped)
        )

        guard case .t1(let arm) = decoded else {
            return XCTFail("selected_option_index should select the second arm")
        }
        XCTAssertEqual(arm.value, 42)
    }
}
