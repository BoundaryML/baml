// Roundtrip coverage for `Baml.generics` — port of python_pydantic2
// `roundtrip_tests/test_generics.py`.
//
// Swift generic structs bind T at compile time; nothing rides the wire
// (the engine infers type args from values — inbound inference). Where
// Python subscripts (`Wrapper[int](value=5)`), Swift infers from the
// argument or spells the specialization explicitly.
import XCTest
import Baml
import BamlBridge

final class TestGenerics: XCTestCase {
    func test_generics_round_trip_wrapper_int() throws {
        let w = Baml.generics.Wrapper<Int>(value: 5)
        XCTAssertEqual(try Baml.generics.round_trip_wrapper_int(w: w), w)
    }

    func test_generics_round_trip_generic_linked_list_int() throws {
        let ll = Baml.generics.GenericLinkedList<Int>(
            value: 1,
            next: Baml.generics.GenericLinkedList<Int>(value: 2, next: nil)
        )
        XCTAssertEqual(try Baml.generics.round_trip_generic_linked_list_int(l: ll), ll)
    }

    func test_generics_round_trip_generic_binary_tree_int() throws {
        let t = Baml.generics.GenericBinaryTree<Int>(value: 1, left: nil, right: nil)
        XCTAssertEqual(try Baml.generics.round_trip_generic_binary_tree_int(t: t), t)
    }

    func test_generics_round_trip_box_int() throws {
        let b = Baml.generics.Box<Int>(value: 3, wrapped: Baml.generics.Wrapper<Int>(value: 4))
        XCTAssertEqual(try Baml.generics.round_trip_box_int(b: b), b)
    }

    func test_generics_round_trip_nested_generics() throws {
        let n = Baml.generics.NestedGenerics(
            ww: Baml.generics.Wrapper(value: Baml.generics.Wrapper(value: 1)),
            wl: Baml.generics.Wrapper(value: [1, 2]),
            wr: Baml.generics.Wrapper(
                value: Baml.generics.GenericLinkedList<Int>(value: 9, next: nil)
            )
        )
        XCTAssertEqual(try Baml.generics.round_trip_nested_generics(n: n), n)
    }

    func test_generics_round_trip_differing_instantiation() throws {
        let d = Baml.generics.DifferingInstantiation(
            list: Baml.generics.GenericLinkedList(
                value: Baml.generics.Wrapper<Int>(value: 1),
                next: nil
            )
        )
        XCTAssertEqual(try Baml.generics.round_trip_differing_instantiation(d: d), d)
    }
}
