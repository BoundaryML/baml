import Foundation
import XCTest
import Baml
import BamlBridge

final class TestUnhandledSpawnErrors: XCTestCase {
    func test_unhandled_spawn_error_uses_host_default() throws {
        if ProcessInfo.processInfo.environment["BAML_SWIFT_UNHANDLED_SPAWN_CHILD"] != nil {
            XCTAssertEqual(try Baml.spawn_unhandled_error(), 1)
            BamlRuntime.shared.shutdown()
            XCTFail("unhandled spawn error did not terminate the process")
            return
        }

        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/xcrun")
        process.arguments = [
            "xctest",
            "-XCTest",
            "TestUnhandledSpawnErrors/test_unhandled_spawn_error_uses_host_default",
            Bundle(for: Self.self).bundleURL.path,
        ]
        process.environment = ProcessInfo.processInfo.environment.merging(
            ["BAML_SWIFT_UNHANDLED_SPAWN_CHILD": "1"]
        ) { _, child in child }
        let stderr = Pipe()
        process.standardError = stderr

        try process.run()
        process.waitUntilExit()

        let output = String(
            decoding: stderr.fileHandleForReading.readDataToEndOfFile(),
            as: UTF8.self
        )
        XCTAssertNotEqual(process.terminationStatus, 0, output)
        XCTAssertTrue(output.contains("user.unhandled_spawn_error"), output)
    }
}
