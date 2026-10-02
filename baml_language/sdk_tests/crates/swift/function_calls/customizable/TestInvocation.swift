import XCTest
import Baml

final class invocation_options: XCTestCase {
    func test_four_call_forms() throws {
        let opts = Baml.BamlOptions(timeoutMs: 1000)
        XCTAssertEqual(try Baml.optional_args_probe(arg0: 1), [1, 5, 99])
        XCTAssertEqual(try Baml.optional_args_probe(arg0: 1, opt1: .value(7)), [1, 7, 99])
        XCTAssertEqual(try Baml.optional_args_probe(arg0: 1, baml: opts), [1, 5, 99])
        XCTAssertEqual(try Baml.optional_args_probe(arg0: 1, opt1: .value(7), baml: opts), [1, 7, 99])
    }

    func test_four_call_forms_async() async throws {
        let opts = Baml.BamlOptions(timeoutMs: 1000)
        let none = try await Baml.optional_args_probe_async(arg0: 1)
        let kwargs = try await Baml.optional_args_probe_async(arg0: 1, opt1: .value(7))
        let controls = try await Baml.optional_args_probe_async(arg0: 1, baml: opts)
        let both = try await Baml.optional_args_probe_async(arg0: 1, opt1: .value(7), baml: opts)
        XCTAssertEqual(none, [1, 5, 99])
        XCTAssertEqual(kwargs, [1, 7, 99])
        XCTAssertEqual(controls, [1, 5, 99])
        XCTAssertEqual(both, [1, 7, 99])
    }

    func test_omitted_argument_is_not_null() throws {
        XCTAssertEqual(try Baml.optional_args_probe(arg0: 1, baml: .init()), [1, 5, 99])
        XCTAssertEqual(try Baml.optional_args_probe(arg0: 1, opt1: nil, baml: .init()), [1, nil, 99])
    }

    func test_explicit_controls_are_applied() {
        let expired = Baml.BamlOptions(timeoutMs: 0)
        XCTAssertThrowsError(try Baml.optional_args_probe(arg0: 1, baml: expired))
        XCTAssertThrowsError(try Baml.optional_args_probe(arg0: 1, opt1: .value(7), baml: expired))
    }

    func test_explicit_controls_are_applied_async() async {
        let expired = Baml.BamlOptions(timeoutMs: 0)
        do {
            _ = try await Baml.optional_args_probe_async(arg0: 1, baml: expired)
            XCTFail("Explicit async invocation controls were ignored")
        } catch {}
        do {
            _ = try await Baml.optional_args_probe_async(arg0: 1, opt1: .value(7), baml: expired)
            XCTFail("Explicit async invocation controls were ignored")
        } catch {}
    }
}
