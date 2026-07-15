// Roundtrip coverage for `Baml.forward_refs` — port of python_pydantic2
// `roundtrip_tests/test_forward_refs.py`.
//
// Not yet ported: rec_list / rec_list_with_other (recursive union
// aliases, Phase 3) and g_node_int (generic classes, Phase 5).
// `Node` (required self-ref) emits and compiles — boxed — but is
// uninhabitable from the host side, exactly like Python's import-only
// treatment.
import XCTest
import Baml

final class TestForwardRefs: XCTestCase {
    func test_round_trip_other() throws {
        let o = Baml.forward_refs.Other(v: 7)
        XCTAssertEqual(try Baml.forward_refs.round_trip_other(o: o), o)
    }

    func test_node_symbol_exists() {
        // Compile-time analog of Python's import-only assertion for the
        // uninhabitable `Node` (required self-reference).
        _ = Baml.forward_refs.Node.self
    }
}
