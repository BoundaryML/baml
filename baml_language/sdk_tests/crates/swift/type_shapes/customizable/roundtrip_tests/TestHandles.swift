// Handle-backed stdlib resources — port of python_pydantic2
// `roundtrip_tests/test_handles.py`. All symbols are stdlib pkg `baml`.
//
// The load-bearing property: opaque `$rust_type` handles round-trip —
// a File's cursor state persists across separate host→engine FFI
// calls because the same engine-side resource rides back in as a
// cloned handle key.
import XCTest
import Foundation
import Baml
import BamlBridge

// 1×1 transparent PNG, same payload as the Python test.
private let pngB64 =
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+M8AAAQEAQB9eIv5AAAAAElFTkSuQmCC"

final class TestHandles: XCTestCase {
    func test_handles_image_from_base64_roundtrips_payload() throws {
        let img = Baml.baml.media.Image(
            _data: try BamlMedia.fromBase64(.image, pngB64, mimeType: "image/png")
        )
        XCTAssertEqual(try img.mime_type(), "image/png")
        XCTAssertEqual(try img.base64(), pngB64)
    }

    func test_handles_open_file_returns_file_handle() throws {
        let path = try makeTempFile(contents: "0123456789")
        let f = try Baml.baml.fs.open(path: path, mode: "r")
        try f.close()
    }

    func test_handles_file_cursor_state_persists_across_calls() throws {
        let path = try makeTempFile(contents: "0123456789")
        let f = try Baml.baml.fs.open(path: path, mode: "r")
        XCTAssertEqual(try f.seek_from(whence: "current", offset: 3), 3)
        XCTAssertEqual(try f.seek_from(whence: "current", offset: 3), 6)
        XCTAssertEqual(try f.seek_from(whence: "start", offset: 0), 0)
        XCTAssertEqual(try f.seek_from(whence: "current", offset: 2), 2)
        XCTAssertEqual(try f.text(), "23456789")
        try f.close()
    }

    private func makeTempFile(contents: String) throws -> String {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("baml-handles-\(UUID().uuidString).txt")
        try contents.write(to: url, atomically: true, encoding: .utf8)
        addTeardownBlock { try? FileManager.default.removeItem(at: url) }
        return url.path
    }
}
