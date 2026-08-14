// Generic functions — port of python_pydantic2 `test_generic_calls.py`
// + `test_generic_inference.py` (the portable union of both).
//
// Swift's static generics collapse Python's three binding surfaces
// (bare inference, `fn[T](...)` subscript, `_types=` kwarg) into one:
// the compiler binds T at the call site, and NOTHING rides the wire —
// the engine re-infers from values (inbound inference). Consequences:
// - explicit-subscript tests port as ordinary typed calls;
// - `_types=`-guard and subscript-arity TypeError tests are compile
//   errors here (unrepresentable, noted per case);
// - divergent-TypeVar cases (`choose(5, "asdf")`) are compile errors
//   (Python unifies dynamically; Swift's T is one type);
// - return-only TypeVars (`parse_as`, `one_type_arg`) are not emitted
//   (need a wire type hint — Python requires `_types=` for them too);
// - reified-metadata assertions (`__pydantic_generic_metadata__`) are
//   static types in Swift: the call compiling IS the assertion.
import XCTest
import Baml
import BamlBridge

private typealias G = Baml.generic_tests

final class TestGenericInference: XCTestCase {
    func test_generic_inference_identity_infers_primitives() throws {
        XCTAssertEqual(try G.identity(x: 5), 5)
        XCTAssertEqual(try G.identity(x: "hi"), "hi")
        XCTAssertEqual(try G.identity(x: true), true)
    }

    func test_generic_inference_identity_infers_user_class() throws {
        let pair = G.StringIntPair(my_string: "a", my_int: 1)
        XCTAssertEqual(try G.identity(x: pair), pair)
    }

    func test_generic_inference_identity_infers_generic_instance() throws {
        let box = G.GenericBox<Int>(value: 5)
        XCTAssertEqual(try G.identity(x: box), box)
        let nested = G.GenericBox(value: G.GenericBox(value: "hello"))
        XCTAssertEqual(try G.identity(x: nested), nested)
    }

    func test_generic_inference_identity_async_infers() async throws {
        let r = try await G.identity_async(x: 7)
        XCTAssertEqual(r, 7)
    }

    func test_generic_inference_identity_null_round_trips() throws {
        let r: Int? = try G.identity(x: Int?.none)
        XCTAssertNil(r)
    }

    func test_generic_inference_make_triple_infers_multiple_typevars() throws {
        let t = try G.make_triple(a: 1, b: ["a", "b"], c: ["k": true])
        XCTAssertEqual(t.first, 1)
        XCTAssertEqual(t.second, ["a", "b"])
        XCTAssertEqual(t.third, ["k": true])
    }

    func test_generic_inference_second_of_infers_from_nested_generic() throws {
        XCTAssertEqual(
            try G.second_of(p: G.GenericPair<Int, String>(first: 1, second: "hi")),
            "hi"
        )
        let pair = G.StringIntPair(my_string: "z", my_int: 9)
        XCTAssertEqual(
            try G.second_of(p: G.GenericPair<Int, G.StringIntPair>(first: 0, second: pair)),
            pair
        )
    }

    func test_generic_inference_read_items_infers_from_instance_wire_args() throws {
        let container = G.ContainerShapes<Int>(
            item: 1, items: [1, 2, 3], by_key: ["k": 4], maybe: nil, mixed: nil
        )
        XCTAssertEqual(try G.read_items(shape: container), [1, 2, 3])
        let empty = G.ContainerShapes<Int>(
            item: 1, items: [], by_key: [:], maybe: nil, mixed: nil
        )
        XCTAssertEqual(try G.read_items(shape: empty), [])
    }

    func test_generic_inference_list_head_infers_from_recursive_generic() throws {
        let list = G.GenericRecursive<Int>(
            value: 7,
            next: G.GenericRecursive<Int>(value: 8, next: nil)
        )
        XCTAssertEqual(try G.list_head(list: list), 7)
    }

    func test_generic_inference_extract_infers_four_typevars_from_nesting() throws {
        let pair = G.GenericPair<G.GenericPair<Int, String>, G.GenericPair<Bool, Double>>(
            first: G.GenericPair<Int, String>(first: 1, second: "a"),
            second: G.GenericPair<Bool, Double>(first: true, second: 1.5)
        )
        XCTAssertEqual(try G.extract(a: pair), "int | string | bool | float")
    }

    func test_generic_inference_choose_infers_unified_typevar() throws {
        XCTAssertEqual(try G.choose(left: 5, right: 6), 5)
        XCTAssertEqual(try G.choose(left: "a", right: "b"), "a")
    }

    func test_generic_inference_wrap_infers_and_returns_generic() throws {
        let w = try G.wrap(x: 5)
        XCTAssertEqual(w.value, 5)
        XCTAssertEqual(w, G.GenericBox<Int>(value: 5))
    }

    func test_generic_inference_tag_or_value_binds_generic_instance() throws {
        // T inferred (Swift + engine agree): int arm selected.
        XCTAssertEqual(try G.tag_or_value(x: .t0(5)), "int")
    }

    func test_generic_inference_union_concrete_sibling_absorbs_value_binds_rust_type() throws {
        // The string value binds the CONCRETE union sibling; T (Int at
        // the Swift level, but never sent) defaults engine-side to the
        // host-only sentinel — identical to Python's bare call.
        XCTAssertEqual(
            try G.tag_or_value(x: BamlUnion2<Int, Swift.String>.t1("hi")),
            "$rust_type"
        )
    }

    func test_generic_inference_union_null_actual_binds_rust_type() throws {
        XCTAssertEqual(
            try G.tag_or_value(x: BamlUnion2<Int, Swift.String>?.none),
            "$rust_type"
        )
    }

    func test_generic_inference_pair_invariant_list_agree_binds() throws {
        XCTAssertEqual(try G.pair(a: [1, 2], b: [3, 4]), "int")
    }

    func test_generic_inference_glue_invariant_and_covariant_agree_binds() throws {
        XCTAssertEqual(try G.glue(bare: 1, arr: [2, 3]), "int")
    }

    func test_generic_inference_triple_choose_three_covariant_join() throws {
        // Heterogeneous T needs a union type at the Swift level; the
        // engine joins the VALUE types: int | string | bool.
        typealias U = BamlUnion3<Int, Swift.String, Bool>
        let rendered = try G.triple_choose(a: U.t0(5), b: U.t1("asdf"), c: U.t2(true))
        XCTAssertTrue(rendered.contains("int"), rendered)
        XCTAssertTrue(rendered.contains("string"), rendered)
        XCTAssertTrue(rendered.contains("bool"), rendered)
    }

    func test_generic_inference_elem_type_homogeneous_array_is_single_type() throws {
        XCTAssertEqual(try G.elem_type(xs: [1, 2, 3]), "int")
    }

    func test_generic_inference_elem_type_heterogeneous_array_unifies() throws {
        let xs: [BamlUnion2<Int, Swift.String>] = [.t0(1), .t1("x")]
        XCTAssertEqual(try G.elem_type(xs: xs), "int | string")
    }

    func test_generic_inference_first_or_empty_list_round_trips_none() throws {
        let r: Int? = try G.first_or(xs: [Int]())
        XCTAssertNil(r)
    }

    func test_generic_inference_first_or_nonempty_infers_element() throws {
        XCTAssertEqual(try G.first_or(xs: [7, 8, 9]), 7)
    }

    func test_generic_inference_values_of_empty_map_round_trips_empty_list() throws {
        XCTAssertEqual(try G.values_of(m: [String: Int]()), [])
    }

    func test_generic_inference_values_of_nonempty_returns_values() throws {
        let values = try G.values_of(m: ["a": 1, "b": 2])
        XCTAssertEqual(values.sorted(), [1, 2])
    }

    func test_generic_inference_maybe_id_present_value_infers() throws {
        XCTAssertEqual(try G.maybe_id(x: 5), 5)
    }

    func test_generic_inference_maybe_id_null_round_trips() throws {
        let r: Int? = try G.maybe_id(x: Int?.none)
        XCTAssertNil(r)
    }

    func test_generic_inference_identity_enum_round_trips() throws {
        XCTAssertEqual(try G.identity(x: G.SomeEnum.VARIANT), .VARIANT)
    }

    // --- methods on generic classes ---

    func test_generic_inference_genericbox_get_infers_class_var_from_receiver() throws {
        XCTAssertEqual(try G.GenericBox<Int>(value: 5).get(), "int")
    }

    func test_generic_inference_genericbox_pair_with_infers_method_typevar() throws {
        XCTAssertEqual(try G.GenericBox<Int>(value: 5).pair_with(other: "hello world"), "int | string")
    }

    func test_generic_inference_generic_static_infers_own_typevar() throws {
        let box = try G.GenericBox<Int>.new(value: 5)
        XCTAssertEqual(box.value, 5)
    }

    func test_generic_inference_named_static_infers_distinct_typevars() throws {
        XCTAssertEqual(
            try G.NamedStatic<Int, Int, Int>.make(d: 1, e: "x"),
            "int | string"
        )
    }

    func test_generic_calls_consume_int_wrapper_baseline() throws {
        XCTAssertEqual(try G.consume_int_wrapper(x: G.GenericBox<Int>(value: 9)), 9)
    }

    // --- reified returns (Swift static types ARE the reification) ---

    func test_generic_calls_make_int_box_reified() throws {
        let box: G.GenericBox<Int> = try G.make_int_box()
        XCTAssertEqual(box.value, 7)
    }

    func test_generic_calls_make_int_container_reified() throws {
        let c = try G.make_int_container()
        XCTAssertEqual(c.item, 1)
        XCTAssertEqual(c.items, [1, 2, 3])
        XCTAssertEqual(c.by_key, ["k": 4])
        XCTAssertNil(c.maybe)
        XCTAssertEqual(c.mixed, .t0(5))
    }

    func test_generic_calls_make_nested_box_reified() throws {
        let outer: G.GenericBox<G.GenericBox<Int>> = try G.make_nested_box()
        XCTAssertEqual(outer.value.value, 9)
    }

    func test_generic_calls_make_int_str_bool_triple_reified() throws {
        let t = try G.make_int_str_bool_triple()
        XCTAssertEqual(t.first, 1)
        XCTAssertEqual(t.second, ["a", "b"])
        XCTAssertEqual(t.third, ["k": true])
    }
}
