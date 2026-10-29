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
        // A string cache, decoded as an int cache: a reference decodes only at
        // the type arguments it came with.
        _ = try Sap._new_parse_cache(T: Swift.String.self) // loads the program
        XCTAssertThrowsError(
            try BamlRuntime.shared.callSync(
                "baml.sap._new_parse_cache",
                args: [],
                typeArgs: [BamlTypeArgument("T", BamlSlot<Swift.String>.own.type)],
                returns: BamlSlot<Sap._ParseCache<Int>>.own
            ) as Sap._ParseCache<Int>
        ) { error in
            guard case BamlDecodeError.typeMismatch(let expected, _) = error else {
                return XCTFail("expected a decode type mismatch, got \(error)")
            }
            XCTAssertEqual(expected, "baml.sap._ParseCache instance at its type arguments")
        }
    }

    func test_generic_references_one_engine_instance_is_one_reference() throws {
        let cache = try Sap._new_parse_cache(T: Int.self)
        XCTAssertEqual(try G.identity(x: cache), cache)
        XCTAssertNotEqual(try Sap._new_parse_cache(T: Int.self), cache)
    }
}
