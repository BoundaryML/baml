// A generic class the runtime keeps to itself (`baml.sap._ParseCache<T>`): an
// instance crosses as a reference at its type arguments, so its methods bind
// `T` from the receiver, and two references to one engine instance are equal.
import XCTest
import Baml
import BamlBridge

private typealias Sap = Baml.baml.sap
private typealias G = Baml.generic_tests

final class TestGenericReferences: XCTestCase {
    func test_generic_references_method_binds_the_class_type_argument_from_its_receiver() async throws {
        let ints = try Sap._new_parse_cache(T: Int.self)
        XCTAssertEqual(try ints._parse_final(json: "41"), 41)
        let asyncResult = try await ints._parse_final_async(json: "41")
        XCTAssertEqual(asyncResult, 41)
        // The same text read at another type argument: the binding is the
        // receiver's, not a default.
        let strings = try Sap._new_parse_cache(T: Swift.String.self)
        XCTAssertEqual(try strings._parse_final(json: "41"), "41")
    }

    func test_generic_references_reference_carries_its_type_arguments() throws {
        // A cache built at a type argument stated by name binds its methods
        // to that argument.
        _ = try Sap._new_parse_cache(T: Swift.String.self) // loads the program
        let strings: Sap._ParseCache<Swift.String> = try BamlRuntime.shared.callSync(
            "baml.sap._new_parse_cache",
            args: [],
            typeArgs: [BamlTypeArgument("T", BamlSlot<Swift.String>.own.type)],
            returns: BamlSlot<Sap._ParseCache<Swift.String>>.own
        )
        XCTAssertEqual(try strings._parse_final(json: "41"), "41")
    }

    func test_generic_references_one_engine_instance_is_one_reference() throws {
        let cache = try Sap._new_parse_cache(T: Int.self)
        XCTAssertEqual(try G.identity(x: cache), cache)
        XCTAssertNotEqual(try Sap._new_parse_cache(T: Int.self), cache)
    }
}
