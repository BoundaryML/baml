// Stdlib entrypoints — port of python_pydantic2
// `test_stdlib_entrypoints.py`: native ($rust_function) and SysOp
// ($rust_io_function) stdlib functions callable as ordinary generated
// entry points.
import XCTest
import Foundation
import Baml

final class TestStdlibEntrypoints: XCTestCase {
    // `baml.sys.argv() -> string[]` is a `$rust_function` ->
    // `FunctionKind::Native`. The fixture host passes no program arguments, so
    // the contents are not worth asserting on — that the call lands and is
    // stable across invocations is.
    func test_stdlib_entrypoints_native_argv_callable_as_entry_point() throws {
        let args = try Baml.baml.sys.argv()
        XCTAssertEqual(args, try Baml.baml.sys.argv())
    }

    func test_stdlib_entrypoints_sysop_fs_exists_callable_as_entry_point() throws {
        XCTAssertTrue(try Baml.baml.fs.exists(path: "."))
    }

    func test_stdlib_entrypoints_compiler_intrinsics_are_not_emitted_as_entry_points() throws {
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
