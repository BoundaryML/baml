// Roundtrip coverage for the cross-namespace routing-rules suite —
// port of python_pydantic2 `roundtrip_tests/test_routing.py`: root
// (`Baml`), `a`, `a.b`, `lorem`, and `ipsum` leaves.
//
// The `baml.http.Response` arms in `lorem` need an engine-minted handle;
// the handle round trip is covered by TestHandles.swift.
import XCTest
import Baml

final class TestRouting: XCTestCase {
    func test_routing_make_foo() throws {
        XCTAssertEqual(try Baml.make_foo(v: 3).v, 3)
    }

    func test_routing_round_trip_foo() throws {
        let f = Baml.Foo(v: 10)
        XCTAssertEqual(try Baml.round_trip_foo(f: f), f)
    }

    func test_routing_round_trip_thing_from_ab() throws {
        let t = Baml.a.b.Thing(v: 1)
        XCTAssertEqual(try Baml.a.b.round_trip_thing_from_ab(t: t), t)
    }

    func test_routing_round_trip_root_foo_from_ab() throws {
        let f = Baml.Foo(v: 2)
        XCTAssertEqual(try Baml.a.b.round_trip_root_foo_from_ab(f: f), f)
    }

    func test_routing_round_trip_deep_thing_from_a() throws {
        let t = Baml.a.b.Thing(v: 4)
        XCTAssertEqual(try Baml.a.round_trip_deep_thing_from_a(t: t), t)
    }

    func test_routing_round_trip_deep_thing_from_lorem() throws {
        let t = Baml.a.b.Thing(v: 5)
        XCTAssertEqual(try Baml.lorem.round_trip_deep_thing_from_lorem(t: t), t)
    }

    func test_routing_round_trip_resume() throws {
        let r = Baml.lorem.Resume(name: "ada", email: nil)
        XCTAssertEqual(try Baml.lorem.round_trip_resume(r: r), r)
    }

    func test_routing_round_trip_root_foo() throws {
        let f = Baml.Foo(v: 6)
        XCTAssertEqual(try Baml.lorem.round_trip_root_foo(f: f), f)
    }

    func test_routing_round_trip_lorem_resume_from_ipsum() throws {
        let r = Baml.lorem.Resume(name: "grace", email: "g@x.com")
        XCTAssertEqual(try Baml.ipsum.round_trip_lorem_resume_from_ipsum(r: r), r)
    }

    func test_routing_round_trip_resume_or_http_response() throws {
        let r = Baml.lorem.Resume(name: "lovelace", email: "a@x.com")
        XCTAssertEqual(try Baml.lorem.round_trip_resume_or_http_response(u: .t0(r)), .t0(r))
    }
}
