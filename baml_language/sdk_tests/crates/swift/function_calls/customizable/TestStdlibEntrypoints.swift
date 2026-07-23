// Stdlib entrypoints — port of python_pydantic2
// `test_stdlib_entrypoints.py`: native ($rust_function) and SysOp
// ($rust_io_function) stdlib functions callable as ordinary generated
// entry points.
import XCTest
import Foundation
import Baml

final class TestStdlibEntrypoints: XCTestCase {
    func test_native_now_ms_callable_as_entry_point() throws {
        XCTAssertGreaterThan(try Baml.baml.sys.now_ms(), 0)
    }

    func test_sysop_fs_exists_callable_as_entry_point() throws {
        XCTAssertTrue(try Baml.baml.fs.exists(path: "."))
    }

    func test_compiler_intrinsics_are_not_emitted_as_entry_points() throws {
        // Python inspects generated .py files; the Swift analog reads
        // the generated sources. `vendor.log.*` and `baml.events.send`
        // are compiler intrinsics and must not surface as entry points.
        let sourcesDir = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()  // Tests/BamlTests
            .deletingLastPathComponent()  // Tests
            .deletingLastPathComponent()  // generated
            .appendingPathComponent("Sources/Baml")

        let vendorSource = try? String(
            contentsOf: sourcesDir.appendingPathComponent("vendor.swift"),
            encoding: .utf8
        )
        if let vendorSource {
            for forbidden in ["func info(", "func debug(", "func warn(", "func error("] {
                XCTAssertFalse(
                    vendorSource.contains(forbidden),
                    "compiler intrinsic leaked into vendor.swift: \(forbidden)"
                )
            }
        }

        let bamlSource = try String(
            contentsOf: sourcesDir.appendingPathComponent("baml.swift"),
            encoding: .utf8
        )
        XCTAssertFalse(
            bamlSource.contains("\"baml.events.send\""),
            "compiler intrinsic baml.events.send leaked into baml.swift"
        )
    }
}
