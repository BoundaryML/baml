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

private let httpBody = "hello from localhost"

/// Minimal blocking HTTP server on an ephemeral port (the Swift analog
/// of Python's http.server fixture).
private final class TinyHTTPServer: @unchecked Sendable {
    private let socketFD: Int32
    let port: UInt16
    private let queue = DispatchQueue(label: "tiny-http")

    init() throws {
        socketFD = socket(AF_INET, SOCK_STREAM, 0)
        var yes: Int32 = 1
        setsockopt(socketFD, SOL_SOCKET, SO_REUSEADDR, &yes, socklen_t(MemoryLayout<Int32>.size))
        var addr = sockaddr_in()
        addr.sin_family = sa_family_t(AF_INET)
        addr.sin_port = 0
        addr.sin_addr.s_addr = inet_addr("127.0.0.1")
        let bindResult = withUnsafePointer(to: &addr) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                bind(socketFD, $0, socklen_t(MemoryLayout<sockaddr_in>.size))
            }
        }
        guard bindResult == 0, listen(socketFD, 4) == 0 else {
            throw NSError(domain: "TinyHTTPServer", code: Int(errno))
        }
        var bound = sockaddr_in()
        var len = socklen_t(MemoryLayout<sockaddr_in>.size)
        _ = withUnsafeMutablePointer(to: &bound) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                getsockname(socketFD, $0, &len)
            }
        }
        port = UInt16(bigEndian: bound.sin_port)

        queue.async { [socketFD] in
            while true {
                let client = accept(socketFD, nil, nil)
                if client < 0 { break }
                var buf = [UInt8](repeating: 0, count: 4096)
                _ = recv(client, &buf, buf.count, 0)
                let response =
                    "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\n"
                    + "Content-Length: \(httpBody.utf8.count)\r\nConnection: close\r\n\r\n"
                    + httpBody
                response.withCString { _ = send(client, $0, strlen($0), 0) }
                close(client)
            }
        }
    }

    var url: String { "http://127.0.0.1:\(port)/" }

    deinit { close(socketFD) }
}

final class TestHandles: XCTestCase {
    func test_image_from_base64_roundtrips_payload() throws {
        let img = Baml.baml.media.Image(
            _data: try BamlMedia.fromBase64(.image, pngB64, mimeType: "image/png")
        )
        XCTAssertEqual(try img.mime_type(), "image/png")
        XCTAssertEqual(try img.base64(), pngB64)
    }

    func test_open_file_returns_file_handle() throws {
        let path = try makeTempFile(contents: "0123456789")
        let f = try Baml.baml.fs.open(path: path, mode: "r")
        try f.close()
    }

    func test_file_cursor_state_persists_across_calls() throws {
        let path = try makeTempFile(contents: "0123456789")
        let f = try Baml.baml.fs.open(path: path, mode: "r")
        XCTAssertEqual(try f.read(n: 3), "012")
        XCTAssertEqual(try f.read(n: 3), "345")
        XCTAssertEqual(try f.seek_from(whence: "start", offset: 0), 0)
        XCTAssertEqual(try f.read(n: 2), "01")
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
