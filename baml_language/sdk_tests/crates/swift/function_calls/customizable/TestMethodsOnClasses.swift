// Static + instance methods — port of python_pydantic2
// `test_methods_on_classes.py`.
//
// test_method_bindings_exist is Python-specific (callable()
// introspection); in Swift the bindings existing IS a compile-time
// fact — every call below fails to compile if one vanishes.
import XCTest
import Baml

final class TestMethodsOnClasses: XCTestCase {
    func test_methods_on_classes_static_create_round_trips() throws {
        let g = try Baml.methods_on_classes.Greeter.create(name: "ada")
        XCTAssertEqual(g.name, "ada")
    }

    func test_methods_on_classes_static_create_async_round_trips() async throws {
        let g = try await Baml.methods_on_classes.Greeter.create_async(name: "grace")
        XCTAssertEqual(g.name, "grace")
    }

    func test_methods_on_classes_instance_who_round_trips() throws {
        let g = try Baml.methods_on_classes.Greeter.create(name: "hopper")
        XCTAssertEqual(try g.who(), "hopper")
    }

    func test_methods_on_classes_instance_who_async_round_trips() async throws {
        let g = try await Baml.methods_on_classes.Greeter.create_async(name: "hopper")
        let who = try await g.who_async()
        XCTAssertEqual(who, "hopper")
    }

    func test_methods_on_classes_instance_greet_with_arg_round_trips() throws {
        let g = try Baml.methods_on_classes.Greeter.create(name: "lovelace")
        XCTAssertEqual(try g.greet(greeting: "hi"), "hi")
    }

    func test_methods_on_classes_instance_greet_async_with_arg_round_trips() async throws {
        let g = try await Baml.methods_on_classes.Greeter.create_async(name: "lovelace")
        let greeting = try await g.greet_async(greeting: "hi")
        XCTAssertEqual(greeting, "hi")
    }
}
