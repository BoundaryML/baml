// Namespace/type reachability — port of python_pydantic2's type_shapes
// `test_main.py` (its pytest half; the pyright half maps to `swift
// build` itself, which type-checks the whole generated package).
//
// Python touches every generated namespace module; Swift touches each
// namespace enum's metatype, which fails to compile if the symbol
// vanishes. Not yet reachable (no supported symbols emitted, so no
// namespace enum exists): `media` (media types, Phase 5).
import XCTest
import Baml

final class TestMain: XCTestCase {
    func test_main_all_namespaces_reachable() {
        _ = Baml.primitives.self
        _ = Baml.enums.self
        _ = Baml.literals.self
        _ = Baml.class_refs.self
        _ = Baml.aliases.self
        _ = Baml.aliases_consumer.self
        _ = Baml.optional.self
        _ = Baml.lists.self
        _ = Baml.maps.self
        _ = Baml.unions.self
        _ = Baml.recursion.self
        _ = Baml.generics.self
        _ = Baml.forward_refs.self
        _ = Baml.complex_models.self
        _ = Baml.lorem.self
        _ = Baml.a.self
    }

    func test_main_root_foo_reachable() throws {
        let f = try Baml.round_trip_foo(f: Baml.Foo(v: 3))
        XCTAssertEqual(f, Baml.Foo(v: 3))
    }

    func test_main_lorem_resume_reachable() {
        _ = Baml.lorem.Resume.self
    }

    func test_main_deep_namespace_thing_reachable() {
        _ = Baml.a.b.Thing.self
    }
}
