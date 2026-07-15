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
    }
}
