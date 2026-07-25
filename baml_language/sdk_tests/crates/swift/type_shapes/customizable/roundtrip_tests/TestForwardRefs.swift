// Roundtrip coverage for `Baml.forward_refs` — port of python_pydantic2
// `roundtrip_tests/test_forward_refs.py`.
//
// Not yet ported: g_node_int (generic classes, Phase 5).
// `Node` (required self-ref) emits and compiles — boxed — but is
// uninhabitable from the host side, exactly like Python's import-only
// treatment.
import XCTest
import Baml

final class TestForwardRefs: XCTestCase {
    func test_forward_refs_round_trip_rec_list() throws {
        // Python value [1, [2, 3]].
        let r: Baml.forward_refs.RecList = .t1([.t0(1), .t1([.t0(2), .t0(3)])])
        XCTAssertEqual(try Baml.forward_refs.round_trip_rec_list(r: r), r)
    }

    func test_forward_refs_round_trip_rec_list_with_other() throws {
        // RecListWithOther = int | Other | RecListWithOther[].
        XCTAssertEqual(
            try Baml.forward_refs.round_trip_rec_list_with_other(r: .t0(1)),
            .t0(1)
        )
        let listy: Baml.forward_refs.RecListWithOther = .t2([.t0(1), .t0(2)])
        XCTAssertEqual(try Baml.forward_refs.round_trip_rec_list_with_other(r: listy), listy)
    }

    func test_forward_refs_round_trip_other() throws {
        let o = Baml.forward_refs.Other(v: 7)
        XCTAssertEqual(try Baml.forward_refs.round_trip_other(o: o), o)
    }

    func test_forward_refs_round_trip_node_symbol_exists() {
        // Compile-time analog of Python's import-only assertion for the
        // uninhabitable `Node` (required self-reference).
        _ = Baml.forward_refs.Node.self
    }
}
