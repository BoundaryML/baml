import Foundation
import XCTest
import Baml
import BamlBridge

/// How long the child may take to die of its trap. Where the trap is
/// delivered the child is gone in a few seconds. In a sandbox that withholds
/// it (seen under an agent's sandbox, where even a three-line `fatalError`
/// program spins in the kernel and never exits) the child has still run the
/// host default, which is what its stderr shows, and is stopped here.
private let childExitDeadline: TimeInterval = 30

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
        // Drain the pipe while the child runs: a child that writes more than
        // the pipe buffers blocks on the write and never exits.
        let output = OutputBox()
        let drained = DispatchSemaphore(value: 0)
        DispatchQueue.global().async {
            output.store(stderr.fileHandleForReading.readDataToEndOfFile())
            drained.signal()
        }
        let deadline = Date().addingTimeInterval(childExitDeadline)
        while process.isRunning && Date() < deadline {
            Thread.sleep(forTimeInterval: 0.1)
        }
        if process.isRunning {
            process.terminate()
        }
        process.waitUntilExit()
        drained.wait()
        let text = String(decoding: output.take(), as: UTF8.self)

        XCTAssertNotEqual(process.terminationStatus, 0, text)
        XCTAssertTrue(text.contains("Fatal error: Unhandled BAML spawn error"), text)
        XCTAssertTrue(text.contains("user.unhandled_spawn_error"), text)
        XCTAssertFalse(text.contains("did not terminate the process"), text)
    }
}

/// The child's stderr, handed from the draining thread to the test.
private final class OutputBox: @unchecked Sendable {
    private let lock = NSLock()
    private var data = Data()

    func store(_ data: Data) {
        lock.lock()
        defer { lock.unlock() }
        self.data = data
    }

    func take() -> Data {
        lock.lock()
        defer { lock.unlock() }
        return data
    }
}
