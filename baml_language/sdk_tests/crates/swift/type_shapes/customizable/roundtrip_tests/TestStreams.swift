// `$stream` companion types as VALUES — port of python_pydantic2
// `roundtrip_tests/test_streams.py`. Companion classes route under
// `Baml.stream_types.<ns>` with the `$stream` suffix stripped
// (mirroring Python's baml_sdk.stream_types).
//
// The `baml.http.Response`-typed round trips stay uncovered here, like
// Python (engine-minted `_body` handle; not host-constructible).
import XCTest
import Baml
import BamlBridge

final class TestStreams: XCTestCase {
    func test_streams_round_trip_resume_stream() throws {
        let r = Baml.stream_types.lorem.Resume(name: "ada", email: nil)
        XCTAssertEqual(try Baml.lorem.round_trip_resume_stream(r: r), r)
    }

    func test_streams_round_trip_root_foo_stream() throws {
        let f = Baml.stream_types.Foo(v: 3)
        XCTAssertEqual(try Baml.lorem.round_trip_root_foo_stream(f: f), f)
    }

    func test_streams_round_trip_box_of_resume_stream() throws {
        let b = Baml.lorem.Box(v: Baml.stream_types.lorem.Resume(name: "grace", email: nil))
        XCTAssertEqual(try Baml.lorem.round_trip_box_of_resume_stream(b: b), b)
    }

    func test_streams_round_trip_resume_or_resume_stream() throws {
        // Union of base and companion; pass the non-stream arm.
        let r = Baml.lorem.Resume(name: "hopper", email: nil)
        XCTAssertEqual(try Baml.lorem.round_trip_resume_or_resume_stream(u: .t0(r)), .t0(r))
    }

    func test_streams_round_trip_resume_or_http_response() throws {
        let r = Baml.lorem.Resume(name: "lovelace", email: "a@x.com")
        XCTAssertEqual(try Baml.lorem.round_trip_resume_or_http_response(u: .t0(r)), .t0(r))
    }
}
