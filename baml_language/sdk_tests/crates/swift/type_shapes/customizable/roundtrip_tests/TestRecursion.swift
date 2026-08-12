// Roundtrip coverage for `Baml.recursion` — port of python_pydantic2
// `roundtrip_tests/test_recursion.py`.
//
// All recursive child fields are optional, so finite values terminate
// with `nil`. On the Swift side every cycle-forming field is
// `@BamlIndirect`-boxed by codegen (structs can't contain themselves);
// the API shape is unchanged.
import XCTest
import Baml

final class TestRecursion: XCTestCase {
    func test_recursion_round_trip_int_binary_tree() throws {
        let t = Baml.recursion.IntBinaryTree(
            value: 1,
            left: Baml.recursion.IntBinaryTree(value: 2, left: nil, right: nil),
            right: nil
        )
        XCTAssertEqual(try Baml.recursion.round_trip_int_binary_tree(t: t), t)
    }

    func test_recursion_round_trip_mutual_recursion() throws {
        let a = Baml.recursion.A(b: Baml.recursion.B(a: nil))
        let b = Baml.recursion.B(a: Baml.recursion.A(b: nil))
        XCTAssertEqual(try Baml.recursion.round_trip_a(a: a), a)
        XCTAssertEqual(try Baml.recursion.round_trip_b(b: b), b)
    }

    func test_recursion_round_trip_scc_t1_t2_t3() throws {
        let t1 = Baml.recursion.T1(via2: Baml.recursion.T2(via1: nil, via3: nil), via3: nil)
        let t2 = Baml.recursion.T2(via1: nil, via3: Baml.recursion.T3(via1: nil, via2: nil))
        let t3 = Baml.recursion.T3(via1: nil, via2: nil)
        XCTAssertEqual(try Baml.recursion.round_trip_t1(t: t1), t1)
        XCTAssertEqual(try Baml.recursion.round_trip_t2(t: t2), t2)
        XCTAssertEqual(try Baml.recursion.round_trip_t3(t: t3), t3)
    }

    func test_recursion_round_trip_scc_t4_t5_t6() throws {
        let t4 = Baml.recursion.T4(via5: Baml.recursion.T5(via4: nil, via6: nil), via6: nil)
        let t5 = Baml.recursion.T5(via4: nil, via6: Baml.recursion.T6(via4: nil, via5: nil))
        let t6 = Baml.recursion.T6(via4: nil, via5: nil)
        XCTAssertEqual(try Baml.recursion.round_trip_t4(t: t4), t4)
        XCTAssertEqual(try Baml.recursion.round_trip_t5(t: t5), t5)
        XCTAssertEqual(try Baml.recursion.round_trip_t6(t: t6), t6)
    }
}
