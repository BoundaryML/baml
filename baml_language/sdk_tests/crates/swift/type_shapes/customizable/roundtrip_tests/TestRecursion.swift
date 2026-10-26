// Roundtrip coverage for `Baml.recursion` — port of python_pydantic2
// `roundtrip_tests/test_recursion.py`.
//
// All recursive child fields are optional, so finite values terminate
// with `nil`. On the Swift side every cycle-forming field is
// `@BamlIndirect`-boxed by codegen (structs can't contain themselves);
// the API shape is unchanged.
import XCTest
import Baml
import BamlBridge

final class TestRecursion: XCTestCase {
    func test_recursion_round_trip_int_binary_tree() throws {
        let t = Baml.recursion.IntBinaryTree(
            value: 1,
            left: Baml.recursion.IntBinaryTree(value: 2, left: nil, right: nil),
            right: nil
        )
        XCTAssertEqual(try Baml.recursion.round_trip_int_binary_tree(t: t), t)
    }

    func test_recursion_int_binary_tree_deeper_than_the_wire_is_refused() throws {
        // A class level costs the wire three of the 100 nested messages it
        // carries (the class, its field's entry, the field's value), and an
        // argument sits two messages below the call: a chain of 32 trees
        // crosses. One more, or far more, is refused before the call with the
        // error the engine raises for an argument it cannot read, never a
        // panic.
        let atTheLimit = leftChain(32)
        XCTAssertEqual(try Baml.recursion.round_trip_int_binary_tree(t: atTheLimit), atTheLimit)
        for nodes in [33, 1000] {
            XCTAssertThrowsError(try Baml.recursion.round_trip_int_binary_tree(t: leftChain(nodes))) { error in
                guard let error = error as? BamlError else {
                    return XCTFail("\(nodes)-deep tree: expected BamlError, got \(error)")
                }
                XCTAssertEqual(error.className, "baml.errors.InvalidArgument", "\(nodes)-deep tree")
                XCTAssertTrue(error.message.contains("100 nested wire messages"), error.message)
            }
        }
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

/// A tree of `nodes` nodes, each the left child of the next.
private func leftChain(_ nodes: Int) -> Baml.recursion.IntBinaryTree {
    (1..<nodes).reduce(Baml.recursion.IntBinaryTree(value: 0, left: nil, right: nil)) { tree, value in
        Baml.recursion.IntBinaryTree(value: value, left: tree, right: nil)
    }
}
